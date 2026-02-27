//! Estructura de Libro
use serde::Deserialize;

/// Representa un autor asociado a un libro.
/// # Campo
/// - `author_id`: Identificador único del autor.
#[derive(Debug, Deserialize)]
pub struct RawAuthor {
    pub author_id: String,
}

/// Representa un libro del dataset.
/// # Campos
/// - `title`: Título del libro.
/// - `ratings_count`: Cantidad de reseñas (puede ser nulo).
/// - `authors`: Lista de autores asociados al libro.
/// - `publisher`: Editorial del libro (puede ser nulo).
#[derive(Debug, Deserialize)]
pub struct Book {
    pub title: String,
    pub ratings_count: Option<String>,
    pub authors: Vec<RawAuthor>,
    pub publisher: Option<String>,
}

impl Book {
    /// Devuelve el `author_id` del primer autor del libro, si existe.
    pub fn author_id(&self) -> Option<String> {
        self.authors.first().map(|a| a.author_id.clone())
    }
}
