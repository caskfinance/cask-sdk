use std::time::Duration;

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use cask_sdk_core as core;

create_exception!(_cask, CaskError, PyException);
create_exception!(_cask, NotReadyError, CaskError);
create_exception!(_cask, CustomerNotFoundError, CaskError);
create_exception!(_cask, FeatureNotFoundError, CaskError);
create_exception!(_cask, WrongTypeError, CaskError);
create_exception!(_cask, AuthError, CaskError);
create_exception!(_cask, NetworkError, CaskError);
create_exception!(_cask, InvalidSnapshotError, CaskError);

fn to_py_err(e: core::Error) -> PyErr {
    let msg = e.to_string();
    match e {
        core::Error::NotReady => NotReadyError::new_err(msg),
        core::Error::CustomerNotFound(_) => CustomerNotFoundError::new_err(msg),
        core::Error::FeatureNotFound(_) => FeatureNotFoundError::new_err(msg),
        core::Error::WrongType { .. } => WrongTypeError::new_err(msg),
        core::Error::Auth => AuthError::new_err(msg),
        core::Error::Network(_) => NetworkError::new_err(msg),
        core::Error::InvalidSnapshot(_) => InvalidSnapshotError::new_err(msg),
    }
}

#[pyclass(module = "cask", get_all, set_all, skip_from_py_object)]
#[derive(Clone)]
struct Config {
    api_key: String,
    base_url: Option<String>,
    poll_interval: f64,
    debug: bool,
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (api_key, base_url=None, poll_interval=30.0, debug=false))]
    fn new(api_key: String, base_url: Option<String>, poll_interval: f64, debug: bool) -> Self {
        Config {
            api_key,
            base_url,
            poll_interval,
            debug,
        }
    }
}

#[pyclass(module = "cask")]
struct Client {
    inner: core::Cask,
}

#[pymethods]
impl Client {
    #[new]
    fn new(config: &Config) -> PyResult<Self> {
        let mut core_config = core::Config::new(config.api_key.clone());
        core_config.debug = config.debug;
        if let Some(url) = &config.base_url {
            core_config.base_url = url.clone();
        }
        core_config.poll_interval = Duration::try_from_secs_f64(config.poll_interval)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Client {
            inner: core::Cask::new(core_config).map_err(to_py_err)?,
        })
    }

    #[pyo3(signature = (timeout=5.0))]
    fn wait_until_ready(&self, py: Python<'_>, timeout: f64) -> PyResult<()> {
        let timeout = Duration::try_from_secs_f64(timeout)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        py.detach(|| self.inner.wait_until_ready(timeout))
            .map_err(to_py_err)
    }

    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    fn check_bool(&self, customer: &str, feature: &str) -> PyResult<bool> {
        self.inner.check_bool(customer, feature).map_err(to_py_err)
    }

    fn check_numeric(&self, customer: &str, feature: &str) -> PyResult<Option<f64>> {
        let limit = self
            .inner
            .check_numeric(customer, feature)
            .map_err(to_py_err)?;
        Ok(limit.map(|l| match l {
            core::Limit::Unlimited => f64::INFINITY,
            core::Limit::Value(v) => v,
        }))
    }

    fn check_enum(&self, customer: &str, feature: &str) -> PyResult<Option<String>> {
        self.inner.check_enum(customer, feature).map_err(to_py_err)
    }

    fn close(&self, py: Python<'_>) {
        py.detach(|| self.inner.close());
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __exit__(
        &self,
        py: Python<'_>,
        _exc_type: Option<Py<PyAny>>,
        _exc_value: Option<Py<PyAny>>,
        _traceback: Option<Py<PyAny>>,
    ) {
        self.close(py);
    }
}

#[pymodule]
fn _cask(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<Client>()?;
    m.add_class::<Config>()?;
    m.add("CaskError", py.get_type::<CaskError>())?;
    m.add("NotReadyError", py.get_type::<NotReadyError>())?;
    m.add("CustomerNotFoundError", py.get_type::<CustomerNotFoundError>())?;
    m.add("FeatureNotFoundError", py.get_type::<FeatureNotFoundError>())?;
    m.add("WrongTypeError", py.get_type::<WrongTypeError>())?;
    m.add("AuthError", py.get_type::<AuthError>())?;
    m.add("NetworkError", py.get_type::<NetworkError>())?;
    m.add("InvalidSnapshotError", py.get_type::<InvalidSnapshotError>())?;
    Ok(())
}
