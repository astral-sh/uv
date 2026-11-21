//! Capture command-support output written by the future under test.
use std::cell::RefCell;
use std::future::Future;

#[derive(Default)]
pub struct CapturedOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

tokio::task_local! {
    static OUTPUT: RefCell<CapturedOutput>;
}

/// Give each invocation its own buffers, including when tests run concurrently.
/// Writes in separately spawned tasks and subprocesses retain their own destinations.
pub async fn capture<F: Future>(command: F) -> (F::Output, CapturedOutput) {
    OUTPUT
        .scope(RefCell::default(), async {
            let result = command.await;
            let CapturedOutput { stdout, stderr } = OUTPUT.with(RefCell::take);
            // A piped command strips terminal styling from both streams.
            let output = CapturedOutput {
                stdout: anstream::adapter::strip_bytes(&stdout).into_vec(),
                stderr: anstream::adapter::strip_bytes(&stderr).into_vec(),
            };
            (result, output)
        })
        .await
}

pub(crate) fn write(bytes: &[u8], stream: fn(&mut CapturedOutput) -> &mut Vec<u8>) -> bool {
    OUTPUT
        .try_with(|output| stream(&mut output.borrow_mut()).extend_from_slice(bytes))
        .is_ok()
}
