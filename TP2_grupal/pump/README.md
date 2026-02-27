# Pump - Surtidor

## Descripción General

El módulo Pump representa un surtidor de combustible que actúa como el cliente en este sistema distribuido. Su responsabilidad es generar transacciones de venta, enviarlas a la Estación de Servicio (GasStation) vía TCP, y escuchar en un puerto separado para recibir la confirmación asíncrona del resultado (aprobada o rechazada).

---

## Estructuras de Datos

### `Pump`
El surtidor mantiene su configuración de red local para poder recibir las respuestas del servidor.

```rust
pub struct Pump {
    response_listener_port: u16,  // Puerto para recibir respuestas de la Gas Station
    local_address: String,        // Dirección completa del pump (IP:PORT)
}
```

#### Flujo de Comunicación
Envío (Síncrono): El Pump se conecta a la GasStation y envía el mensaje TransactionMessage.

Espera (Asíncrona): El Pump no bloquea la conexión de envío esperando el resultado. En su lugar, levanta un TcpListener en su propio hilo.

Recepción: Cuando la Estación (y el Regional) procesan la venta, la Estación se conecta al Pump (usando local_address) y le entrega el TransactionResponse.

### Cómo Ejecutar 

**Argumentos requeridos:**
```bash
cargo run --bin pump -- \
  --station-addr <ip:port> \    # Dirección de la Gas Station
  --response-port <port> \      # Puerto local para recibir respuestas
  --local-addr <ip>             # IP local del pump
```

**Ejemplo:**
```bash
cargo run --bin pump -- \
  --station-addr 127.0.0.1:8080 \
  --response-port 5001 \
  --local-addr 127.0.0.1
```

**Funcionalidades:**
- Genera transacciones automáticamente cada 5 segundos
- Envía transacciones a la Gas Station
- Recibe respuestas asíncronas indicando si cada transacción fue aprobada o rechazada
- Muestra feedback claro:
  - Transacciones aprobadas
  - Transacciones rechazadas con motivo (límite excedido, cuenta no encontrada, etc.)

### Pruebas 
El módulo incluye pruebas unitarias para verificar su funcionalidad:
test_generate_transaction: Verifica que las transacciones se generen correctamente con los datos proporcionados.
test_send_transaction_failure: Verifica que se manejen correctamente los errores al intentar enviar una transacción a una dirección inválida.

Para correrlas, dentro del directorio de este servicio simplemente corremos
`cargo test`