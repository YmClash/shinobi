//! Script de build pour la compilation des fichiers Protobuf.
//!
//! Utilise `tonic-prost-build` pour générer le code Rust à partir
//! des définitions `.proto` du protocole Ninpo.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::compile_protos("../../proto/shinobi.proto")?;
    Ok(())
}
