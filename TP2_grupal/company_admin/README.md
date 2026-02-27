# Administrador

Actor "cliente" del sistema.
- Actúa como una aplicación de comandos donde el cliente puede realizar queries.
- El cliente inicia sesión en su cuenta y a partir de ahí todos los comandos, estarán relacionados solo a dicha cuenta.
- Si el cliente ejecuta "LOGOUT", cerrará su sesión.
- Luego de cerrar su sesión el cliente puede iniciar sesión con la misma o alguna otra cuenta (siempre y cuando cuente con la contraseña).

## ¿Qué mensajes va a contemplar?

Para cada comando, se parseará usándo la primera palabra del comando como keyword, el resto, parámetros.

- Inicio de sesión 

Durante esta operación se setean los parámetros internos del cliente para las subsiguientes queries.

```bash
LOGIN <company_id>
```

- Cierre de sesión

Luego de esta operación los parámetros internos del cliente se vuelven nulos y no se pueden realizar otras operaciones.

```
LOGOUT
```

- Alta de cuenta

Una vez que la compañía registre su nombre, se envía el primer mensaje la SV central para el alta.
Donde este response con el código de éxito/fallo y id que le corresponde a la compañía.

Esta operación equivale a hacer un `LOGIN`

```bash
REG <company_name> <account_limit>
```

- Alta de tarjeta

Durante esta operación se agrega una tarjeta a la cuenta asociada. El servidor central verifica las restricciones
y response con el id de la tarjeta si fue exitosamente.

```bash
CARD <card_limit>
```

- Update

Permite realizar los cambios en los datos de la cuenta asociada. Acciones tales como actualizar nombre,
actualizar límite de cuenta y actualizar límite de tarjeta indicada.

```bash
UPD -n <new_account_name>
```

```bash
UPD -l <new_account_limit>
```

```bash
CARD -c <card_id> <new_card_limit>
```

- Consultas

Para consultar los datos de la cuenta y tarjetas asociadas.

```bash
QUERY -d
```

Para consulta de transacciones asociadas a la cuenta

```bash
QUERY -a <start_date> <end_date>
```

Para consulta de cuenta filtrada por tarjeta

```bash
QUERY -c <card_id> <start_date> <end_date>
```

Notar que en caso de haber una fecha faltante se considerara la única disponible como límite inferior, y
en caso de no haber ninguna fecha, se consultarán los datos históricos.


## ¿Con quién se va a relacionar?

### Servidor central

- Alta de cuenta

```rust
struct RegisterAccountMessage {
    company_name: String,
    limit: f64,
}
```

```rust
struct DataResponseMessage {
    code: u8, // 0
    status: u8, // 0 OK, -1, error
    id: u64, // New account id
}
```

- Modificación de datos de cuenta,

```rust
struct UpdateDataMessage {
    id: u64,
    new_company_name: String, // Vacío si no quiero cambiar el nombre de la companía
    card_id: u64, // 0 si el límite es para la cuenta
    new_limit: f64,
}
```

```rust
struct DataResponseMessage {
    code: u8, // 1
    status: u8, // 0 OK, -1, error
    id: u64 // Vacío
}
```

- Baja de cuenta,

```rust
struct DeleteDataMessage {
    id: u64,
    card_id: u64, // 0 si el "borrado" es para la cuenta
}
```

```rust
struct DataResponseMessage {
    code: u8, // 2
    status: u8, // 0 OK, -1, error
    id: u64, // Vacío
}
```

- Alta de tarjetas,

```rust
struct RegisterCardMessage {
    company_id: u64,  // Id de la compania a registrar la tarjeta
}
```

```rust
struct DataResponseMessage {
    code: u8, // 3
    status: u8, // 0 OK, -1, error
    id: u64, // New card id
}
```

- Modificación de datos de tarjeta,

```rust
// Comparte mensaje con modificación de datos de cuenta.
struct UpdateDataMessage {
    id: u64,
    new_company_name: String, // Vacío
    card_id: u64, // 0 si el límite es para la cuenta
    new_limit: u64,
}
```

```rust
struct DataResponseMessage {
    code: u8, // 4
    status: u8, // 0 OK, -1, error
    id: u64, // Vacío
}
```

- Baja de tarjetas,

```rust
struct DeleteDataMessage {
    id: u64, // Id de la cuenta
    card_id: u64, // 0 si el límite es para la cuenta
}
```

```rust
struct DataResponseMessage {
    code: u8, // 5
    status: u8, // 0 OK, -1, error
    id: u64, // Vacío
}
```


- Pedir estado actual de la cuenta.

```rust
struct QueryAccountData {
    id: u64, // Id de la cuenta
    card_id: u64, // 0 si la query es para la cuenta
    start_date: Timestamp, // Ambos 0 si busco históricos
    end_date: Timesteamp,
}
```

```rust
struct QueryResponseMessage {
    id: u64,
    card_id: u64, // 0 si la información es de la cuenta
    company_name: String,
    limit: f64,
    available_credit: f64,
    data: Vec<Data>
}
```

```rust
struct Data {
    date: Timestamp,
    gas_station_id: u64,
    pump_id: u8,
    card_id: u64, // Una sola tarjeta asociada a un conductor
    gas_price: f64,
    gas_amount: f64, // In liters
    total: f64,
    fees: f64,
}
```

## Estructuras relacionadas

### Enums

- ResponseCode

Enum hecho para mantener controlado la cantidad de códigos disponibles para usar en los mensajes de respuesta.