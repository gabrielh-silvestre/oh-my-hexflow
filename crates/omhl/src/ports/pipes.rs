//! Leitoras dos canos do runner: interno da S5.

use std::io::{ErrorKind, Read};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

/// Espera pelos canos depois de colher o líder: um descendente fugido pode segurá-los abertos.
pub const PIPE_GRACE: Duration = Duration::from_secs(2);

const READ_BLOCK: usize = 8 * 1024;

pub enum Chunk {
    Data(Vec<u8>),
    Capped,
}

/// Lê `source` numa thread solta até o EOF, mandando no máximo `max_bytes` bytes; o excedente é
/// lido e descartado, para o comando nunca travar com o cano cheio.
pub fn spawn_reader(mut source: impl Read + Send + 'static, max_bytes: u64) -> Receiver<Chunk> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut block = vec![0; READ_BLOCK];
        let mut room = max_bytes;
        let mut capped = false;
        loop {
            let read = match source.read(&mut block) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            let kept = read.min(usize::try_from(room).unwrap_or(usize::MAX));
            if kept > 0 {
                let _ = sender.send(Chunk::Data(block[..kept].to_vec()));
                room -= kept as u64;
            }
            if kept < read && !capped {
                capped = true;
                let _ = sender.send(Chunk::Capped);
            }
        }
    });
    receiver
}

/// Acumula até o `Sender` cair ou até `deadline`; `truncated` = teto atingido ou prazo vencido.
pub fn collect(receiver: &Receiver<Chunk>, deadline: Instant) -> (Vec<u8>, bool) {
    let mut bytes = Vec::new();
    let mut capped = false;
    loop {
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(chunk) => absorb(chunk, &mut bytes, &mut capped),
            Err(RecvTimeoutError::Disconnected) => return (bytes, capped),
            Err(RecvTimeoutError::Timeout) => {
                // prazo vencido: entra só o que já chegou ao canal
                while let Ok(chunk) = receiver.try_recv() {
                    absorb(chunk, &mut bytes, &mut capped);
                }
                return (bytes, true);
            }
        }
    }
}

fn absorb(chunk: Chunk, bytes: &mut Vec<u8>, capped: &mut bool) {
    match chunk {
        Chunk::Data(data) => bytes.extend(data),
        Chunk::Capped => *capped = true,
    }
}
