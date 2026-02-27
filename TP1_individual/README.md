[![Review Assignment Due Date](https://classroom.github.com/assets/deadline-readme-button-22041afd0340ce965d47ae6ef1cefeee28c7c493a6346c4f15d667ab976d596c.svg)](https://classroom.github.com/a/VT9bB7CI)

# Informe del Trabajo Práctico 1 - Programación concurrente

## Nombre y Padrón
- **Nombre y Apellido**: Lara Converso
- **Padrón**: 107632

## Descripción del Dataset
- **Link**: https://www.kaggle.com/datasets/pypiahmad/goodreads-book-reviews1?select=goodreads_books.json
- **Descripción**: El dataset contiene información sobre libros y autores, incluyendo campos como título, cantidad de reseñas, editorial, etc. Fue interpretado como un conjunto de registros estructurados en formato JSON.

Se utilizaron los archivos "goodreads_books.json" de 9GB y "goodreads_books_authors.json" de 100MB; En cada uno de los archivos, una linea json era una entrada de datos. Por ejemplo para el del libros una línea es un libro.

## Instrucciones de Ejecución
En primer lugar se deben descargar los archivos del dataset a utilizar y ubicarlos en una ruta conocida, para luego poder utilizarla en la ejecución.

Compilación:
   ```bash
   cargo build
   ```
Ejecución:
   ```bash
   cargo run -- <ruta_books.json> <ruta_authors.json> <cantidad_hilos>
   ```
   - `<ruta_books.json>`: Ruta al archivo JSON con los datos de libros.
   - `<ruta_authors.json>`: Ruta al archivo JSON con los datos de autores.
   - `<cantidad_hilos>`: Número de hilos a utilizar.
   Por ejemplo, con 4 hilos
   ```
   cargo run -- src/data/goodreads_books.json src/data/goodreads_book_authors.json 4
   ```
 
## Implementación

### Loader
El módulo `loader` se encarga de la carga de datos desde archivos JSON. Implementa dos funciones principales:
- **Carga de autores:** Lee el archivo `goodreads_books_authors.json` y lo convierte en una estructura de datos manejable en Rust, que luego servirá durante el procesamiento para las transformaciones.
- **Carga de libros:** Utiliza lectura en bloques y procesamiento paralelo para manejar eficientemente archivos grandes como `goodreads_books.json`. Esto se logra mediante el uso de la biblioteca Rayon, que permite dividir el trabajo en múltiples hilos usando la funcion de par_bridge. 

### Tipos de Datos
Se definieron estructuras específicas para representar los datos de libros y autores:
- **Book:** Representa un libro con campos como título, cantidad de reseñas, editorial y autor. Esta estructura se deserializa directamente desde el JSON utilizando `serde`.
- **Author:** Representa un autor con información como nombre y su id, para luego asociarlo a los libros correspondientes. También se deserializa desde el JSON y se utiliza para realizar las transformaciones relacionadas con los autores.

### Decisiones de Diseño

Utilicé `HashMap` para agrupar y contar autores y editoriales, y `Vec` para listas de libros y resultados. Estas estructuras son rápidas y fáciles de manejar en Rust, y además funcionan bien con procesamiento paralelo. Así, puedo dividir el trabajo entre varios hilos y juntar los resultados de forma eficiente usando el modelo fork-join.

### Transformaciones Aplicadas
**Top Autores por Reseñas:**

Se calculan los 5 autores con más reseñas totales y se listan los 3 libros más reseñados de cada uno.

Para lograr esto, se utiliza un enfoque de fork-join. Paso a paso:
   1. _Fork:_ Los datos de libros se dividen en bloques y se procesan en paralelo por múltiples hilos usando el modelo fork-join.
   2. _Procesamiento paralelo:_ Cada hilo agrupa los libros por autor y suma la cantidad de reseñas para su bloque.
   3. _Join:_ Se combinan los resultados parciales de todos los hilos para obtener el total de reseñas por autor.
   4. _Selección y ordenamiento:_ Se seleccionan los 5 autores con más reseñas y, para cada uno, se listan sus 3 libros más reseñados.

_Formato del resultado:_
   ```
   Autor: [Nombre del autor] - [Cantidad total de reseñas]
   Libro: [Título] - [Cantidad de reseñas]
   ```
   Por ejemplo:
   ```
   Terry Pratchett — 138341 reviews totales
      Thief of Time (Discworld, #26; Death, #5) — 42783 reviews
      Carpe Jugulum (Discworld #23; Witches #6) — 36291 reviews 
      Wintersmith (Discworld, #35; Tiffany Aching, #3) — 34927 reviews
   ```

**Top Editoriales por Cantidad de Libros:**

Se identifican las 5 editoriales con más libros publicados.
Este cálculo también utiliza un enfoque de fork-join. Paso a paso:
1. _Fork:_ Los datos de libros se dividen en bloques y se procesan en paralelo por varios hilos.
2. _Procesamiento paralelo:_ Cada hilo cuenta la cantidad de libros publicados por editorial en su bloque.
3. _Join:_ Se combinan los conteos parciales de todos los hilos para obtener el total de libros por editorial.
4. _Selección y ordenamiento:_ 
   Se seleccionan las 5 editoriales con más libros publicados.

_Formato del resultado:_
   ```
   [Nombre de la editorial] - [Cantidad de libros]
   ```
   Por ejemplo:
   ```
   Oxford University Press, USA — 507 libros
   ```

En el archivo `output.txt` se puede visualizar un ejemplo del output esperado del programa completo con el archivo JSON de libros entero de 9GB, y con ambas transformaciones aplicadas y el tiempo tomado para cada sección. 

## Tests
En la carpeta `tests` se encuentran los tests correspondientes para las transformaciones.
En cada test se encuentra la llamada a la transformacion que se testea con un archivo de 100MB. 
Para ejecutarlos, utilizar el comando `cargo test`

## Análisis de Performance
Se realizaron pruebas con diferentes cantidades de hilos para evaluar el impacto en el tiempo de ejecución y el uso de memoria.
Para medir la memoria utilizada, utilicé el comando `gnu-time` y el dato en la tabla es el de uso máximo de memoria o RSS (Resident Set Size).
 Los resultados se presentan a continuación:

| Cantidad de Hilos | Tiempo de Ejecución Total(s) | Uso de memoria(MB) |
|-------------------|-------------------------|-------------------------|
| 1                 | 189.726456542s          |  2013MB                 | 
| 2                 | 96.107297292s           |  3494MB                 | 
| 4                 | 61.104647667s           |       4656MB            | 

## Conclusiones y Mejoras
**Conclusiones**:
   - La concurrencia con 2 hilos reduce significativamente el tiempo de ejecución en comparación con 1 hilo, pero no es tan eficiente como con 4 hilos.
   - El uso de memoria aumenta con el número de hilos, lo cual tiene sentido dado que al generar más hilos, se crean más estructuras de datos y buffers en memoria para manejar el procesamiento concurrente. Esto permite dividir el trabajo entre los hilos, pero a costa de un mayor consumo de recursos.

**Mejoras**:
   - Para reducir la carga de trabajo y el consumo de memoria de cada hilo, creo que una buena opcion sería filtrar los libros que no tienen reseñas o que tienen mas de x cantidad de reseñas antes de iniciar el procesamiento de las transformaciones. 




