use shared_resources::utils::get_timestamp;
use std::env;
use std::process::{Child, Command};

/// Log informativo con timestamp
fn log_info(msg: &str) {
    println!("[{}] [LAUNCHER] [INFO] {}", get_timestamp(), msg);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // pruebo 3 regionales por defecto
    let count: u8 = if args.len() > 1 {
        args[1].parse().expect("el argumento debe ser un número")
    } else {
        3
    };

    log_info(&format!("Levantando {} regionales...", count));
    log_info("Redundancia en centrales puertos 9000, 9001, 9002");

    let mut children: Vec<Child> = Vec::new();
    let base_port = 8000;

    for i in 1..=count {
        let port = base_port + (i as u16);
        let id = i;

        log_info(&format!("Lanzando regional #{} en puerto {}", id, port));

        let child = Command::new("cargo")
            .arg("run")
            .arg("-p")
            .arg("regional_server")
            .arg("--bin")
            .arg("regional_server")
            .arg("--")
            .arg(id.to_string())
            .arg(port.to_string())
            .arg("127.0.0.1:9000")
            .arg("127.0.0.1:9001")
            .arg("127.0.0.1:9002")
            .spawn()
            .expect("Falló al iniciar el proceso hijo");
        children.push(child);
    }

    log_info("Presiona Ctrl+C para detener todos los procesos");

    for mut child in children {
        let _ = child.wait();
    }
}
