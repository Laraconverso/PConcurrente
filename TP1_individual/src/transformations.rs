//! Implementa las transformaciones concurrentes sobre el dataset usando el modelo fork-join.

use crate::book::Book;
use rayon::prelude::*;
use std::collections::HashMap;

///Constantes para la cantidad de resultados a retornar
const TOP_FIVE: usize = 5;
const TOP_BOOKS_COUNT: usize = 3;

///Defino los tipos de datos que retorna cada transformación
type AuthorBooks = HashMap<String, Vec<(String, u32)>>;
type TopAuthors = Vec<(String, u32, Vec<(String, u32)>)>;

/// Devuelve los 5 autores con más reseñas totales, junto a sus 3 libros más reseñados.
/// Se agrupan los libros por autor, se suman las reseñas por cada uno.
/// Se ordenan los autores por total de reseñas y se toman los 5 mejores.
/// Retorna esos 5 mejores en conjunto con los 3 libros mas reseñados.
///
/// # Parametros
/// - `books`: Referencia a la lista de libros.
/// - `authors`: Mapa de IDs de autor a nombre de autor.
///
/// # Retorno
/// Un vector con tuplas de (nombre del autor, total de reseñas, lista de sus 3 libros más reseñados).
pub fn top_authors_by_reviews(books: &[Book], authors: &HashMap<String, String>) -> TopAuthors {
    let author_books: AuthorBooks = books
        .par_iter()
        .filter_map(|book| {
            let count_str = book.ratings_count.as_ref()?;
            let binding = book.author_id();
            let author_id = binding.as_ref()?;
            let count = count_str.parse::<u32>().ok()?;
            let author_name = authors.get(author_id)?.clone();
            Some((author_name, book.title.clone(), count))
        })
        .fold(
            HashMap::new,
            |mut accumulator: HashMap<String, Vec<(String, u32)>>, (author, title, count)| {
                accumulator.entry(author).or_default().push((title, count));
                accumulator
            },
        )
        .reduce(
            HashMap::new,
            |mut a: HashMap<String, Vec<(String, u32)>>, b| {
                for (key, value) in b {
                    a.entry(key).or_default().extend(value);
                }
                a
            },
        );

    let mut author_data: Vec<_> = author_books
        .into_par_iter()
        .map(|(author, mut books)| {
            books.sort_by(|a, b| b.1.cmp(&a.1));
            let total_reviews: u32 = books.iter().map(|(_, count)| count).sum();
            (
                author,
                total_reviews,
                books.into_iter().take(TOP_BOOKS_COUNT).collect::<Vec<_>>(),
            )
        })
        .collect();

    author_data.par_sort_by(|a, b| b.1.cmp(&a.1));

    author_data.into_iter().take(TOP_FIVE).collect()
}

/// Devuelve las 5 editoriales con más libros publicados.
/// Se agrupan los libros por editorial, se cuentan y se ordenan.
/// Luego se toman las 5 editoriales con más libros.
///
/// # Parametros
/// - `books`: Referencia a la lista de libros.
///
/// # Retorno
/// Un vector con tuplas de (nombre de la editorial, cantidad de libros publicados).
pub fn top_publishers_by_book_count(books: &[Book]) -> Vec<(String, u32)> {
    let publisher_map: HashMap<String, u32> = books
        .par_iter()
        .filter_map(|book| {
            book.publisher
                .as_ref()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
        })
        .fold(HashMap::new, |mut accumulator, publisher| {
            *accumulator.entry(publisher).or_insert(0) += 1;
            accumulator
        })
        .reduce(HashMap::new, |mut a, b| {
            for (key, value) in b {
                *a.entry(key).or_insert(0) += value;
            }
            a
        });

    let mut publisher_data: Vec<_> = publisher_map
        .into_par_iter()
        .map(|(publisher, count)| (publisher, count))
        .collect();

    publisher_data.par_sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });

    publisher_data.into_iter().take(TOP_FIVE).collect()
}
