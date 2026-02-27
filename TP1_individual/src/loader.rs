//! Funciones para cargar datos desde JSON.

use crate::author::Author;
use crate::book::Book;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

/// Carga los datos del JSON de autores y los transforma en un `HashMap`.
///
/// # Parametros
/// - `path`: Ruta al archivo JSON de autores.
///
/// # Retorno
/// Un `HashMap` donde la clave es el `author_id` y el valor es el nombre del autor.
pub fn load_authors_map(path: &str) -> HashMap<String, String> {
    let file = File::open(path).expect("No se pudo abrir el archivo de autores");
    let reader = BufReader::new(file);
    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Author>(&line).ok())
        .map(|author| (author.author_id, author.name))
        .collect()
}

/// Carga los datos de libros desde un archivo JSON utilizando lectura en bloques y procesamiento paralelo.
///
/// # Parametros
/// - `path`: Ruta al archivo JSON de libros.
///
/// # Retorno
/// Un `Vec<Book>` con todos los libros deserializados del archivo.
pub fn load_books_streaming(path: &str) -> Vec<Book> {
    let file = File::open(path).expect("No se pudo abrir el archivo de libros");
    let reader = BufReader::new(file);

    reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .expect("Error al leer las líneas del archivo")
        .par_chunks(1000)
        .flat_map_iter(|chunk| {
            chunk
                .iter()
                .filter_map(|line| serde_json::from_str::<Book>(line).ok())
        })
        .collect()
}
