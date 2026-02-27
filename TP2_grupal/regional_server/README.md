# Servidor Regional

Actúa como una barrera de contención y agregador para el Servidor Central. Recibe lotes de transacciones de múltiples Estaciones de Servicio, valida la disponibilidad de saldo en su caché local y gestiona la reserva de fondos mediante un protocolo de Two-Phase Commit (2PC). Posee lógica de alta disponibilidad para reportar las ventas: si la conexión con el Servidor Central falla, intenta rotar entre las direcciones de réplicas configuradas hasta lograr el envío.

## Con quién se va a relacionar

### Estación de Servicio

Protocolo de Two-Phase Commit (2PC) sobre TCP. La estación envía un lote, el regional "vota" si puede procesarlo (reservando saldo) y espera la confirmación final.

#### Fase 1 (Prepare): recibe el lote de ventas


    struct BulkTransactionMessage {
        bulk_id: u64,
        transactions: Vec<TransactionMessage>
    }


#### Fase 1 (Voto): responde si hay saldo


    BulkPrepareResponse {
        bulk_id: u64,
        vote: bool, // true si al menos una transacción fue aceptada
        accepted_transactions: Vec<TransactionMessage>,
        rejected_transactions: Vec<(TransactionMessage, TransactionStatus)>,
    }


#### Fase 2 (Commit/Abort): recibe la decisión final de la Estación

    GlobalCommit {
        bulk_id: u64
    },
    
    GlobalAbort {
        bulk_id: u64
    },


- Confirmación: avisa que los datos se persistieron correctamente


    struct BulkCommittedAck {
        bulk_id: u64,
    }


### Servidor Central

- Reporte: Utiliza un hilo dedicado (CentralReporter) que consume las transacciones confirmadas y las envía al Servidor Central. Implementa un mecanismo de failover; recibe una lista de direcciones IP y rota entre ellas si la conexión falla.


    struct RegionalBatchMessage {
        regional_id: u8,
        transactions: Vec<TransactionMessage>,
    }


- Actualizaciones: Recibe mensajes de control desde el Central para mantener su caché actualizada.


    enum CentralSyncMessage {
        AccountUpdate { ... }, // Crea o actualiza cuenta y límites
        CardUpdate { ... },    // Actualiza límites de tarjetas
        AccountDeleted { ... },
        FullSync { ... },      // Solicitud de estado inicial
        RemoveAccount { ... }
    }


## Estructuras

### Estructura Principal (en ``server.rs``)

Orquesta los componentes del servidor. Inicia el hilo CentralReporter para el envío asíncrono de ventas y el TcpListener para recibir a las estaciones.

    pub struct RegionalServer {
        id: u8,
        port: u16,
        accounts: AccountsManager, // Gestor de estado concurrente
        central_sender: Sender<Vec<TransactionMessage>>, // Canal hacia el reporter
    }


### Gestor de Cuentas (en ``accounts.rs``)

Maneja el estado financiero de la región. Utiliza RwLock para permitir múltiples lecturas concurrentes (validación de saldo) y Mutex para la gestión de transacciones pendientes en la fase intermedia del 2PC.

    pub struct AccountsManager {
        // Caché de lectura rápida para validaciones
        cache: Arc<RwLock<HashMap<u64, AccountState>>>,
        
        // Almacén temporal para la Fase 1 del 2PC
        pending_bulks: Arc<Mutex<HashMap<u64, Vec<TransactionMessage>>>>,
    }

### Estado de Cuenta (en ``accounts.rs``)

Representación en memoria de la cuenta para la toma de decisiones local.

    pub struct AccountState {
        pub account_id: u64,
        pub name: String,
        pub limit: f64,
        pub current_spending: f64, // Gasto confirmado
        pub reserved: f64,         // Gasto reservado (Fase 1)
        pub card_limits: HashMap<u64, f64>,
        pub active: bool,
    }
