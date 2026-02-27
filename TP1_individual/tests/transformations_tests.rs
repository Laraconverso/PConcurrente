use tp1_laraconverso::loader::{load_authors_map, load_books_streaming};
use tp1_laraconverso::transformations::{top_authors_by_reviews, top_publishers_by_book_count};

#[test]
fn test_top_authors_by_reviews() {
    let books_path = "./src/data/goodreads_books_100MB.json";
    let authors_path = "./src/data/goodreads_book_authors_100MB.json";

    let books = load_books_streaming(books_path);
    let authors = load_authors_map(authors_path);

    let result = top_authors_by_reviews(&books, &authors);

    assert_eq!(result.len(), 5); // Verificar que se devuelven los 5 autores principales
    for author in &result {
        assert!(author.2.len() <= 3); // Cada autor debe tener como máximo 3 libros principales
    }
}

#[test]
fn test_top_publishers_by_book_count() {
    let books_path = "./src/data/goodreads_books_100MB.json";

    let books = load_books_streaming(books_path);

    let result = top_publishers_by_book_count(&books);

    assert!(!result.is_empty()); // Asegurarse de que se procesaron datos
    assert_eq!(result.len(), 5); // Verificar que hay 5 editoriales
}
