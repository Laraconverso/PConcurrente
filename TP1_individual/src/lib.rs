//! # Proyecto de Procesamiento de Datos
//!
//! Implementación de una aplicación concurrente en Rust para procesar grandes volúmenes de datos de libros y autores utilizando el modelo Fork-Join.
//!
//! ## Módulos
//! - `author`: Define la estructura y funciones relacionadas con los autores.
//! - `book`: Define la estructura y funciones relacionadas con los libros.
//! - `loader`: Funciones para cargar y transformar los datos desde archivos JSON.
//! - `transformations`: Implementa las transformaciones concurrentes sobre el dataset usando el modelo fork-join.
pub mod author;
pub mod book;
pub mod loader;
pub mod transformations;
