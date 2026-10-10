//! Décodeur Server-Sent Events (SSE) partagé par les adaptateurs LLM distants.
//!
//! ## Phase 42-pre — Hub d'Agents 🧠🔌
//! Gemini (`?alt=sse`) et les API OpenAI-compatibles streament en SSE.
//! Ce module normalise ces flux vers `LlmStreamChunk`.
//!
//! ## Point critique — fragmentation TCP (« l'Amnésie du Stream »)
//! Un chunk `reqwest` peut se terminer n'importe où : au milieu d'un champ
//! (`da` | `ta: [DONE]`), d'une ligne, ou même **au milieu d'un caractère
//! UTF-8 multi-octets** (emoji, accents). Le décodeur accumule donc des
//! **octets bruts** et ne décode en UTF-8 que des lignes complètes.
//!
//! Spécification : <https://html.spec.whatwg.org/multipage/server-sent-events.html>

use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::warn;

use domain::ports::llm_service::LlmStreamChunk;

/// Décodeur SSE incrémental (tolérant à la fragmentation réseau).
///
/// Retourne le contenu des champs `data:` d'un événement complet
/// (délimité par une ligne vide). Les lignes `data:` multiples d'un même
/// événement sont jointes par `\n`. Les commentaires (`:`) et les autres
/// champs (`event:`, `id:`, `retry:`) sont ignorés.
#[derive(Debug, Default)]
pub struct SseDecoder {
    /// Octets reçus mais pas encore terminés par `\n`.
    buffer: Vec<u8>,
    /// Lignes `data:` de l'événement en cours.
    pending_data: Vec<String>,
}

impl SseDecoder {
    /// Crée un décodeur vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingère un chunk réseau et retourne les payloads `data` des événements complets.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=pos).collect();
            line.pop(); // '\n'
            if line.last() == Some(&b'\r') {
                line.pop(); // CRLF
            }
            // La ligne est complète → frontières UTF-8 garanties.
            let line = String::from_utf8_lossy(&line);
            self.process_line(&line, &mut events);
        }

        events
    }

    /// Termine le flux : traite la dernière ligne sans `\n` final
    /// et émet l'événement en attente (serveurs qui ferment sans ligne vide).
    pub fn finish(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let rest = std::mem::take(&mut self.buffer);
            let line = String::from_utf8_lossy(&rest);
            let line = line.trim_end_matches('\r').to_string();
            self.process_line(&line, &mut events);
        }
        self.dispatch(&mut events);
        events
    }

    fn process_line(&mut self, line: &str, events: &mut Vec<String>) {
        if line.is_empty() {
            self.dispatch(events);
            return;
        }
        if line.starts_with(':') {
            return; // Commentaire / heartbeat.
        }
        let (field, value) = match line.find(':') {
            Some(i) => {
                let v = &line[i + 1..];
                (&line[..i], v.strip_prefix(' ').unwrap_or(v))
            }
            None => (line, ""),
        };
        if field == "data" {
            self.pending_data.push(value.to_string());
        }
    }

    fn dispatch(&mut self, events: &mut Vec<String>) {
        if !self.pending_data.is_empty() {
            events.push(self.pending_data.join("\n"));
            self.pending_data.clear();
        }
    }
}

/// Interprétation d'un payload SSE par un adaptateur.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SsePayload {
    /// Fragment de texte à émettre.
    Token(String),
    /// Fin explicite du flux (ex: `[DONE]` OpenAI).
    Done,
    /// Payload sans texte (métadonnées, usage, rôle…).
    Ignore,
}

/// Issue du pompage d'un flux SSE.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PumpOutcome {
    /// Flux terminé normalement (`Done` ou fin du body).
    Completed,
    /// Le `Receiver` a été droppé (client déconnecté).
    ClientGone,
    /// Erreur de lecture réseau (déjà signalée via le canal).
    Failed,
}

/// Lit un body SSE et émet les tokens via `tx`.
///
/// `parse` convertit un payload `data` en `SsePayload`. Une erreur de parsing
/// est loggée et la ligne ignorée (robustesse face aux payloads inattendus).
/// N'émet **pas** le `Done` final : l'appelant l'émet avec ses métriques.
pub(crate) async fn pump_sse<F>(
    response: reqwest::Response,
    tx: &mpsc::Sender<LlmStreamChunk>,
    provider: &str,
    mut parse: F,
) -> PumpOutcome
where
    F: FnMut(&str) -> Result<SsePayload, String>,
{
    let mut decoder = SseDecoder::new();
    let mut byte_stream = response.bytes_stream();

    loop {
        let (payloads, body_ended) = match byte_stream.next().await {
            Some(Ok(bytes)) => (decoder.push(&bytes), false),
            Some(Err(e)) => {
                let _ = tx
                    .send(LlmStreamChunk::Error {
                        message: format!("{provider} — stream read error: {e}"),
                    })
                    .await;
                return PumpOutcome::Failed;
            }
            // Body épuisé : on vide le reliquat du tampon.
            None => (decoder.finish(), true),
        };

        for payload in payloads {
            match parse(&payload) {
                Ok(SsePayload::Token(content)) if !content.is_empty() => {
                    if tx.send(LlmStreamChunk::Token { content }).await.is_err() {
                        return PumpOutcome::ClientGone;
                    }
                }
                Ok(SsePayload::Done) => return PumpOutcome::Completed,
                Ok(_) => {}
                Err(e) => {
                    warn!(provider, error = %e, "⚠️ Hub LLM — payload SSE invalide (ignoré)");
                }
            }
        }

        if body_ended {
            return PumpOutcome::Completed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_simple_event() {
        let mut d = SseDecoder::new();
        assert_eq!(d.push(b"data: hello\n\n"), vec!["hello"]);
    }

    #[test]
    fn reassembles_field_split_across_chunks() {
        // « L'Amnésie du Stream » : `da` | `ta: [DONE]`
        let mut d = SseDecoder::new();
        assert!(d.push(b"da").is_empty());
        assert!(d.push(b"ta: [DO").is_empty());
        assert_eq!(d.push(b"NE]\n\n"), vec!["[DONE]"]);
    }

    #[test]
    fn reassembles_utf8_char_split_across_chunks() {
        let full = "data: ninja 🥷 é\n\n".as_bytes();
        // Coupe au milieu de l'emoji (4 octets).
        let cut = full.iter().position(|&b| b == 0xF0).unwrap() + 2;
        let mut d = SseDecoder::new();
        assert!(d.push(&full[..cut]).is_empty());
        assert_eq!(d.push(&full[cut..]), vec!["ninja 🥷 é"]);
    }

    #[test]
    fn handles_crlf_and_comments_and_other_fields() {
        let mut d = SseDecoder::new();
        let out = d.push(b": keep-alive\r\nevent: message\r\nid: 1\r\ndata: x\r\n\r\n");
        assert_eq!(out, vec!["x"]);
    }

    #[test]
    fn joins_multiline_data() {
        let mut d = SseDecoder::new();
        assert_eq!(d.push(b"data: a\ndata: b\n\n"), vec!["a\nb"]);
    }

    #[test]
    fn multiple_events_in_one_chunk() {
        let mut d = SseDecoder::new();
        assert_eq!(d.push(b"data: 1\n\ndata: 2\n\ndata: 3"), vec!["1", "2"]);
        assert_eq!(d.finish(), vec!["3"]);
    }

    #[test]
    fn data_without_space_after_colon() {
        let mut d = SseDecoder::new();
        assert_eq!(d.push(b"data:{\"a\":1}\n\n"), vec!["{\"a\":1}"]);
    }
}
