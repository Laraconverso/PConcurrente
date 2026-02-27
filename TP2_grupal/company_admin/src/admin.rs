use crate::commands::command::Command;
use crate::types::SyncPair;
use chrono::Local;
use shared_resources::utils::AccountId;
use std::error::Error;
use std::io::{Write, stdin, stdout};
use std::sync::mpsc::Sender;

pub struct CompanyAdministrator {
    company_id: Option<AccountId>,
    output_sender: Sender<Vec<u8>>,
}

impl CompanyAdministrator {
    pub fn new(output_sender: Sender<Vec<u8>>) -> Self {
        Self {
            company_id: None,
            output_sender,
        }
    }

    pub fn run(&mut self, pair: SyncPair) {
        let (lock, cvar) = &*pair;
        loop {
            match parse_and_read_cmd() {
                Ok(cmd) => {
                    if !cmd.apply(&self.output_sender, &mut self.company_id) {
                        continue;
                    }
                    match cmd {
                        Command::LogOut() => {
                            self.company_id = None;
                        }
                        Command::Exit() => {
                            break;
                        }
                        _ => {
                            let mut got_res = lock.lock().unwrap(); // Espero mientras no haya respuesta del sv central en Messenger
                            while !got_res.0 {
                                got_res = cvar.wait(got_res).unwrap();
                            }
                            if let Some(id) = got_res.1.take() {
                                self.company_id = Some(id);
                            }
                            got_res.0 = false;
                            continue;
                        }
                    }
                }
                _ => eprintln!("[ADMIN] [ERROR] Unknown command"),
            }
        }
    }
}

fn parse_and_read_cmd() -> Result<Command, Box<dyn Error>> {
    print!("> ");
    stdout().flush()?;
    let mut aux = String::new();
    if let Err(e) = stdin().read_line(&mut aux) {
        eprintln!(
            "[{}] [ADMIN] [ERROR] Error while getting the command: {}",
            Local::now().format("%H:%M:%S%.3f"),
            e
        )
    }
    Ok(Command::parse(aux))
}
