//! Define la estructura Author
use serde::Deserialize;

/// Representa un autor del dataset.
/// # Campos
/// - `author_id`: Identificador único del autor.
/// - `name`: Nombre del autor.
#[derive(Debug, Deserialize)]
pub struct Author {
    pub author_id: String,
    pub name: String,
}
