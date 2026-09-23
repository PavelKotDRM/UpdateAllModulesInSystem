//! Общий HTTP-клиент для сетевых проверок обновлений.

use crate::updater::UpdaterError;
use reqwest::blocking::Client;
use std::sync::OnceLock;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

static HTTP_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();

/// Возвращает общий клиент с ограничениями времени подключения и запроса.
pub(super) fn http_client() -> Result<&'static Client, UpdaterError> {
    match HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .http1_only()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(client) => Ok(client),
        Err(error) => Err(UpdaterError::Message(crate::tr!(
            crate::localization::current_language(),
            updater,
            create_http_client_error,
            error = error
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::http_client;

    #[test]
    fn reuses_configured_http_client() {
        let first = http_client().expect("HTTP client should be created");
        let second = http_client().expect("HTTP client should be reused");

        assert!(std::ptr::eq(first, second));
    }
}
