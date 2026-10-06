use std::sync::Arc;
use std::time::Duration;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use cask_sdk as core;

/// Errors carry a `code` property: NotReady, CustomerNotFound, FeatureNotFound,
/// WrongType, Auth, Network or InvalidSnapshot.
fn to_js_err(e: core::Error) -> napi::Error<String> {
    let code = match e {
        core::Error::NotReady => "NotReady",
        core::Error::CustomerNotFound(_) => "CustomerNotFound",
        core::Error::FeatureNotFound(_) => "FeatureNotFound",
        core::Error::WrongType { .. } => "WrongType",
        core::Error::Auth => "Auth",
        core::Error::Network(_) => "Network",
        core::Error::InvalidSnapshot(_) => "InvalidSnapshot",
    };
    napi::Error::new(code.to_string(), e.to_string())
}

fn invalid_arg(msg: &str) -> napi::Error<String> {
    napi::Error::new("InvalidArgument".to_string(), msg.to_string())
}

fn secs(value: f64, name: &str) -> Result<Duration, String> {
    Duration::try_from_secs_f64(value).map_err(|_| invalid_arg(&format!("invalid {name}")))
}

#[napi(object)]
pub struct CaskOptions {
    /// Defaults to http://127.0.0.1:8080.
    pub base_url: Option<String>,
    /// Seconds between snapshot refreshes. Defaults to 30.
    pub poll_interval: Option<f64>,
    /// Print debug messages to stderr. Defaults to false.
    pub debug: Option<bool>,
}

#[napi]
pub struct Cask {
    inner: Arc<core::Cask>,
}

#[napi]
impl Cask {
    #[napi(constructor)]
    pub fn new(api_key: String, options: Option<CaskOptions>) -> Result<Self, String> {
        let mut config = core::Config::new(api_key);
        if let Some(options) = options {
            if let Some(url) = options.base_url {
                config.base_url = url;
            }
            if let Some(interval) = options.poll_interval {
                config.poll_interval = secs(interval, "pollInterval")?;
            }
            if let Some(debug) = options.debug {
                config.debug = debug;
            }
        }
        Ok(Cask {
            inner: Arc::new(core::Cask::new(config).map_err(to_js_err)?),
        })
    }

    /// Resolves when the first snapshot has loaded; rejects after `timeout` seconds (default 5).
    #[napi(ts_return_type = "Promise<void>")]
    pub fn wait_until_ready(&self, timeout: Option<f64>) -> Result<AsyncTask<WaitUntilReady>, String> {
        Ok(AsyncTask::new(WaitUntilReady {
            inner: self.inner.clone(),
            timeout: secs(timeout.unwrap_or(5.0), "timeout")?,
        }))
    }

    #[napi]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    #[napi]
    pub fn check_bool(&self, customer: String, feature: String) -> Result<bool, String> {
        self.inner.check_bool(&customer, &feature).map_err(to_js_err)
    }

    /// Returns null if not granted, and Infinity for unlimited.
    #[napi]
    pub fn check_numeric(&self, customer: String, feature: String) -> Result<Option<f64>, String> {
        let limit = self
            .inner
            .check_numeric(&customer, &feature)
            .map_err(to_js_err)?;
        Ok(limit.map(|l| match l {
            core::Limit::Unlimited => f64::INFINITY,
            core::Limit::Value(v) => v,
        }))
    }

    /// Returns null if not granted.
    #[napi]
    pub fn check_enum(&self, customer: String, feature: String) -> Result<Option<String>, String> {
        self.inner.check_enum(&customer, &feature).map_err(to_js_err)
    }

    #[napi]
    pub fn close(&self) {
        self.inner.close();
    }
}

pub struct WaitUntilReady {
    inner: Arc<core::Cask>,
    timeout: Duration,
}

impl Task for WaitUntilReady {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<Self::Output> {
        self.inner
            .wait_until_ready(self.timeout)
            .map_err(|e| napi::Error::new(Status::GenericFailure, e.to_string()))
    }

    fn resolve(&mut self, _env: Env, _output: Self::Output) -> Result<Self::JsValue> {
        Ok(())
    }
}
