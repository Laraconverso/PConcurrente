use std::env;
use std::time::Instant;
use tp1_laraconverso::loader::{load_authors_map, load_books_streaming};
use tp1_laraconverso::transformations::{top_authors_by_reviews, top_publishers_by_book_count};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Uso: cargo run -- <books_path> <authors_path> <threads>");
        return;
    }

    let books_path = &args[1];
    let authors_path = &args[2];
    let threads: usize = args[3].parse().expect("Debe ser un número");

    let start = Instant::now();

    // Configurar el pool global al inicio del programa
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .expect("No se pudo configurar el pool global");

    let authors_map = load_authors_map(authors_path);
    let books = load_books_streaming(books_path);
    let duration_load = start.elapsed();

    //Primera transformacion
    let start_transf = Instant::now();
    let top_authors = top_authors_by_reviews(&books, &authors_map);
    println!("\nTop 5 autores por cantidad total de reviews:");
    for (author, total, books) in top_authors {
        println!("{} — {} reviews totales", author, total);
        for (title, count) in books {
            println!("   {} — {} reviews", title, count);
        }
    }
    let duration_first = start_transf.elapsed();

    //Segunda transformacion
    let start_second = Instant::now();
    let top_publishers = top_publishers_by_book_count(&books);
    println!("\nTop 5 editoriales con más libros publicados:");
    for (publisher, count) in top_publishers {
        println!("{} — {} libros", publisher, count);
    }
    let duration_second = start_second.elapsed();

    let total_duration = start.elapsed();
    println!("\nHilo(s): {:?}", threads);
    println!("Tiempo de carga de datos: {:?}", duration_load);
    println!(
        "Tiempo de ejecución primera transformacion : {:?}",
        duration_first
    );
    println!(
        "Tiempo de ejecución segunda transformacion: {:?}",
        duration_second
    );
    println!("Tiempo de ejecución TOTAL: {:?}", total_duration);
}
