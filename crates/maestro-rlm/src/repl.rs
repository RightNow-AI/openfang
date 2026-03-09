//! Python REPL execution environment via PyO3.
//!
//! Implements the primary execution environment for the RLM agent using PyO3
//! to embed a Python interpreter inside the Rust process.
//!
//! ## Design
//!
//! - `execute(code)`: wraps code in a stdout-capture harness (io.StringIO),
//!   runs it via `exec()` in a persistent globals dict, returns captured output.
//!   Python exceptions are caught and returned as strings ("PYTHON_ERROR: ...")
//!   so the LLM can self-correct without panicking.
//!
//! - `set_variable(name, value)`: injects a named string into the persistent
//!   Python globals so it survives across `execute()` calls.
//!
//! ## PyO3 Pattern
//!
//! `Python::with_gil(|py| { ... })` — the embedding direction (Rust calls Python).
//! The `auto-initialize` feature starts the interpreter on first use.
//!
//! ## Security Warning
//!
//! No sandboxing. For production, wrap in OpenFang docker_sandbox.

use std::ffi::CString;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

use crate::{ExecutionEnvironment, RlmError};

/// Python execution environment using PyO3.
///
/// Maintains a persistent Python globals dict so variables set via
/// `set_variable()` and state created during `execute()` survive across
/// iterations of the RLM loop.
pub struct Pyo3Executor {
    globals: Py<PyDict>,
}

impl Pyo3Executor {
    pub fn new() -> Self {
        let globals = Python::with_gil(|py| {
            let d = PyDict::new(py);
            if let Ok(io) = py.import("io") {
                let _ = d.set_item("io", io);
            }
            if let Ok(sys) = py.import("sys") {
                let _ = d.set_item("sys", sys);
            }
            d.unbind()
        });
        Self { globals }
    }
}

impl Default for Pyo3Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEnvironment for Pyo3Executor {
    /// Execute Python code and return captured stdout.
    ///
    /// Wraps the user's code in a stdout-capture harness that redirects
    /// `sys.stdout` to an `io.StringIO` buffer, executes via `exec()` in
    /// the persistent globals dict, restores stdout, and returns the buffer.
    fn execute(&self, code: &str) -> Result<String, RlmError> {
        Python::with_gil(|py| {
            let globals = self.globals.bind(py);

            // {:?} on &str produces a Rust debug string which is also a valid
            // Python string literal for typical code content.
            let code_repr = format!("{:?}", code);

            // Build the stdout-capture wrapper. We use unique names (_rlm_*)
            // to avoid colliding with user-defined variables.
            // IMPORTANT: Python is indentation-sensitive. Each line must start
            // at column 0 (or exactly 4 spaces for the try/except/finally body).
            // We build the string line-by-line to make the indentation explicit.
            let wrapper = [
                "import io as _io_mod, sys as _sys_mod",
                "_rlm_buf = _io_mod.StringIO()",
                "_sys_mod.stdout = _rlm_buf",
                "try:",
                &format!("    exec({code_repr}, globals())"),
                "except Exception as _rlm_exc:",
                "    print('PYTHON_ERROR: ' + str(_rlm_exc))",
                "finally:",
                "    _sys_mod.stdout = _sys_mod.__stdout__",
                "_rlm_output = _rlm_buf.getvalue()",
            ].join("\n");

            let c_wrapper = CString::new(wrapper)
                .map_err(|e| RlmError::PythonError(format!("CString error: {e}")))?;
            if let Err(e) = py.run(&c_wrapper, Some(globals), None) {
                return Err(RlmError::PythonError(e.to_string()));
            }

            let output: String = globals
                .get_item("_rlm_output")
                .map_err(|e| RlmError::PythonError(e.to_string()))?
                .and_then(|v| v.extract::<String>().ok())
                .unwrap_or_default();

            Ok(output.trim_end().to_string())
        })
    }

    /// Set a named string variable in the persistent Python globals.
    ///
    /// This is how the RLM agent loads the user's prompt into the REPL as
    /// a Python variable (typically `context`) without putting the full
    /// content in the LLM's context window.
    fn set_variable(&self, name: &str, value: &str) -> Result<(), RlmError> {
        Python::with_gil(|py| {
            let globals = self.globals.bind(py);
            let py_value = PyString::new(py, value);
            globals
                .set_item(name, py_value)
                .map_err(|e| RlmError::PythonError(e.to_string()))
        })
    }

    fn env_type(&self) -> &str {
        "python-pyo3"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_print() {
        let exec = Pyo3Executor::new();
        let out = exec.execute("print('hello from python')").unwrap();
        assert_eq!(out, "hello from python");
    }

    #[test]
    fn test_execute_arithmetic() {
        let exec = Pyo3Executor::new();
        let out = exec.execute("print(2 + 2)").unwrap();
        assert_eq!(out, "4");
    }

    #[test]
    fn test_execute_multiline() {
        let exec = Pyo3Executor::new();
        let out = exec.execute("x = 10\ny = 20\nprint(x + y)").unwrap();
        assert_eq!(out, "30");
    }

    #[test]
    fn test_execute_exception_returns_error_string() {
        let exec = Pyo3Executor::new();
        // Exceptions must NOT panic — they surface as "PYTHON_ERROR: ..."
        // so the LLM loop can feed them back to the model for self-correction.
        let out = exec.execute("raise ValueError('test error')").unwrap();
        assert!(
            out.contains("PYTHON_ERROR") || out.contains("ValueError"),
            "Expected error prefix, got: {out}"
        );
    }

    #[test]
    fn test_set_variable_and_read_back() {
        let exec = Pyo3Executor::new();
        exec.set_variable("context", "the magic number is 42").unwrap();
        let out = exec.execute("print(context)").unwrap();
        assert_eq!(out, "the magic number is 42");
    }

    #[test]
    fn test_set_variable_used_in_computation() {
        let exec = Pyo3Executor::new();
        exec.set_variable("data", "hello world foo bar baz").unwrap();
        let out = exec.execute("print(len(data.split()))").unwrap();
        assert_eq!(out, "5");
    }

    #[test]
    fn test_state_persists_between_calls() {
        let exec = Pyo3Executor::new();
        exec.execute("counter = 0").unwrap();
        exec.execute("counter += 1").unwrap();
        exec.execute("counter += 1").unwrap();
        let out = exec.execute("print(counter)").unwrap();
        assert_eq!(out, "2");
    }

    #[test]
    fn test_env_type() {
        assert_eq!(Pyo3Executor::new().env_type(), "python-pyo3");
    }
}
