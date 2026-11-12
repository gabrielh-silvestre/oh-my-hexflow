use std::sync::mpsc;

use ::omhl::runner::{CommandRunner, RunError, RunErrorKind, RunReport, RunSpec};

/// Runner falso: uma resposta fixa por argv. Cada chamada manda o `RunSpec` recebido ao
/// `Receiver` que o `new` devolve, para o teste conferir argv, cwd, ambiente e a ordem.
pub struct FakeRunner {
    responses: Vec<(Vec<String>, Result<RunReport, RunError>)>,
    sent: mpsc::Sender<RunSpec>,
}

impl FakeRunner {
    pub fn new(
        responses: Vec<(Vec<String>, Result<RunReport, RunError>)>,
    ) -> (FakeRunner, mpsc::Receiver<RunSpec>) {
        let (sent, received) = mpsc::channel();
        (FakeRunner { responses, sent }, received)
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, spec: &RunSpec) -> Result<RunReport, RunError> {
        // o teste pode não ler os pedidos; o envio sem Receiver não muda a resposta
        let _ = self.sent.send(spec.clone());
        self.responses
            .iter()
            .find(|(argv, _)| *argv == spec.argv)
            .map(|(_, response)| response.clone())
            .unwrap_or_else(|| {
                Err(RunError {
                    kind: RunErrorKind::Spawn,
                    message: "unexpected argv".to_owned(),
                })
            })
    }
}
