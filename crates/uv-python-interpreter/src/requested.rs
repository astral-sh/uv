use uv_python_types::PythonRequest;

use crate::Interpreter;

/// An interpreter together with the resolved request used to select it.
///
/// Retaining the request allows environment creation to respect constraints discovered in version
/// files or project and script metadata.
#[derive(Debug, Clone)]
pub struct RequestedInterpreter {
    interpreter: Interpreter,
    request: PythonRequest,
}

impl RequestedInterpreter {
    /// Retain the resolved request that selected an interpreter.
    pub fn new(interpreter: Interpreter, request: PythonRequest) -> Self {
        Self {
            interpreter,
            request,
        }
    }

    /// Return the resolved request.
    pub fn request(&self) -> &PythonRequest {
        &self.request
    }

    /// Consume the requested interpreter, discarding its request.
    pub fn into_interpreter(self) -> Interpreter {
        self.interpreter
    }
}
