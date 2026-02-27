pub mod heartbeat;
pub mod messages;
pub mod ring_election;

// Re-exportar NodeInfo para facilitar su uso
pub use ring_election::NodeInfo;
