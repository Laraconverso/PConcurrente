# Estación de Servicio

## Descripción General

La Estación de Servicio (GasStation) actúa como un Coordinador en un sistema distribuido. Su responsabilidad principal es recibir y almacenar transacciones (TransactionMessage) de los Surtidores (Pumps) y, una vez que el buffer alcanza un límite de tamaño o tiempo, iniciar un protocolo de Confirmación en Dos Fases (2PC) con el Servidor Regional para asegurar la atomicidad y consistencia de los lotes de transacciones (bulks).

El servicio opera con tres hilos principales: un Receptor para escuchar a los Surtidores, un Empaquetador para crear lotes (bulks) y un Enviador que coordina el 2PC con el Regional.

---

## Interacciones

### Surtidor

- **Recibe Transacciones**: La Estación de Servicio escucha las transacciones enviadas por los Surtidores.
- **Confirma Transacciones**: Después de procesarlas, envía un mensaje de confirmación al Surtidor.

### Estructura de la estacion de servicio
```rust
pub struct GasStation {
    // Configuración general: IPs, puertos, límites.
    config: GasStationConfig, 
    // Buffer para acumular ventas recibidas. Protegido por Arc<Mutex>.
    buffer: Arc<Mutex<Vec<TransactionMessage>>>, 
    // Contador para generar IDs únicos de Bulk.
    bulk_id_counter: Arc<Mutex<u64>>,
    // Cola de Bulks pendientes de enviar o confirmar (esperando 2PC).
    bulk_queue: Arc<Mutex<VecDeque<(u64, Vec<TransactionMessage>)>>>,
    // Mapeo para rastrear a dónde responder: TransactionId -> Dirección del Pump.
    transaction_routing: Arc<Mutex<HashMap<Uuid, String>>>, 
}
```
La configuracion es cargada desde la linea de comandos 
```rust
pub struct GasStationConfig {
   pub pump_listener_port: u16, // Puerto para escuchar los Pumps
   pub regional_address: String,
   pub regional_port: u16,      // Puerto para enviar el Bulk al Regional
   pub bulk_limit: usize,       // Límite de transacciones para iniciar el Bulk
   pub bulk_timeout_secs: u64,  // Tiempo máximo de espera para iniciar un Bulk
}
```
### Interacciones y Protocolo

Se recibe un mensaje que utiliza tipos de shared_resources y el ID de transacción es un UUID. Además, la dirección del Surtidor se utiliza para la respuesta.

```rust
//! Mensaje enviado desde el Surtidor a la Estación de Servicio para registrar una venta.
struct TransactionMessage {
   transaction_id: Uuid, // ID único de la venta (UUID).
   account_id: u64,
   card_id: u64,
   amount: f64,
   timestamp: u64,
   station_id: u16,
   pump_address: Option<String>, // Dirección del pump (usada para la respuesta).
}
```
Notificación de Resultado (Estación --> Surtidor)

La estación responde a la dirección del surtidor almacenada en transaction_routing usando un TransactionResponse, que encapsula un TransactionStatus detallado (Aceptada, Rechazada por Límite, No Encontrada, etc.).

2. Protocolo con el Servidor Regional (2PC)

El hilo Enviador gestiona la comunicación con el Servidor Regional usando una única conexión TCP para cada Bulk.


### Cómo ejecutar?

1. Iniciar la Estación de Servicio:  
La Estación de Servicio escucha las transacciones de los Surtidores y se comunica con el Servidor Regional.

`cargo run --bin gas_station -- --pump-port <port> --regional-addr <ip> --regional-send-port <port> --bulk-limit <num> --bulk-timeout <secs>`

   Ejemplo:
   ` cargo run --bin gas_station -- --pump-port 8080 --regional-addr 127.0.0.1 --regional-send-port 9000 --bulk-limit 15 --bulk-timeout 45`

2. Enviar Transacciones desde un Surtidor:
   Usa el Surtidor para generar y enviar transacciones a la Estación de Servicio.
3. Servidor Regional 
Debemos asegurarno de que el Servidor Regional esté en ejecución y configurado para aceptar conexiones desde la Estación de Servicio

### Pruebas 
Se incluyen pruebas unitarias para verificar el correcto funcionamiento de la Estación de Servicio. 

Para ejecutarlas corremos dentro del directorio del servicio
`cargo test` 