use futures_util::StreamExt;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT_RANGES, AUTHORIZATION, CONTENT_TYPE, RANGE, USER_AGENT,
};
use url::{Position, Url};

use crate::utils::{
    color::Coloralex,
    log_utils::{seconds_as_string, size_as_string},
};
use std::{
    cmp::min,
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use serde_json::Value;
use tokio::fs;


use tokio::time::Instant;
use crate::utils::file_utils::get_sha1_checksum;


pub struct RestServer;

const REPORT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
pub enum SendType {
    GET,
    POST,
    PUT,
    FILE,
}

enum ContentType {
    Json,
    Protobuf,
    Form,
}

pub struct DownloadStats {
    pub stats: HashMap<String, (u64, u64)>,
    start_time: Instant,
    last_report: Instant,
    last_size: u64,
    pub eta: u64,
    pub download_count: usize
}

impl Default for DownloadStats { fn default() -> Self { Self::new() } }

pub fn report_progress(stats: &mut DownloadStats) {
    let (size, total) = stats
        .stats
        .values()
        .fold((0, 0), |acc: (u64, u64), x| (acc.0 + x.0, acc.1 + x.1));
    let now = Instant::now();
    let since_beginning = now.duration_since(stats.start_time).as_millis() as f64 / 1000.0;
    let since_last = now.duration_since(stats.last_report).as_millis() as f64 / 1000.0;
    if since_last >= REPORT_INTERVAL.as_secs() as f64 {
        let diff = if size > stats.last_size { size - stats.last_size } else { 0 };
        let speed_last = diff / (since_last as u64);
        let speed_average = size / (since_beginning as u64);
        let speed_estimate = std::cmp::max(speed_last, speed_average);
        let estimate = {
            if speed_estimate == 0 {
                "Unknown".to_string()
            } else {
                seconds_as_string((total - size) as f64 / (speed_estimate as f64))
            }
        };
        log::info!(
            "{} {}{}{}{} {}{}{} {}",
            "Downloaded".yellow(true),
            size_as_string(size).cyan(true),
            "/".yellow(true),
            size_as_string(total).cyan(true),
            ", speed:".yellow(true),
            size_as_string(speed_estimate).cyan(true),
            "/s".cyan(true),
            ", time remaining:".yellow(true),
            estimate.cyan(true)
        );
        stats.eta = ((total - size) as f64 / (speed_estimate as f64)) as u64;

        stats.last_report = now;
        stats.last_size = size;
    }
    if size > 0 && size == total {
        log::info!(
            "{} {} {} {}",
            "Finished downloading".yellow(true),
            size_as_string(size).cyan(true),
            "in".yellow(true),
            seconds_as_string(since_beginning).cyan(true)
        );
    }
}


impl DownloadStats {
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
            start_time: Instant::now(),
            last_report: Instant::now(),
            last_size: 0,
            eta: 0,
            download_count: 0
        }
    }
    pub fn inc_download_count(&mut self){ self.download_count += 1; }
    pub fn dec_download_count(&mut self){ self.download_count -= 1; }
    pub fn update_entry(&mut self, key: String, value: u64, size: u64) {
        let k = match self.stats.get_mut(&key) {
            None => {
                self.stats.insert(key.clone(), (value, size));
                self.stats.get_mut(&key).unwrap()
            }
            Some(x) => x,
        };
        *k = (value, size);
        if value > 0 && value == size {
            log::info!("File {} finished downloading.", key);
        }
        report_progress(self);
    }
}

impl RestServer {
    pub fn get(url: &Url, authorization: Option<String>) -> Result<(String, u16), (String, u16)> {
        let client = reqwest::blocking::Client::new();
        let body = client
            .get(url.as_str())
            .headers(RestServer::construct_headers(
                authorization,
                ContentType::Json,
            ))
            .send();
        RestServer::get_response_text(body)
    }

    pub async fn put(url: &Url, authorization: Option<String>, body: &serde_json::Value) -> Result<(String, u16), (String, u16)> {
        let client = reqwest::Client::new();
        log::info!("putting body {}", body.to_string());
        let body = client
            .put(url.as_str())
            .headers(RestServer::construct_headers(
                authorization,
                ContentType::Json,
            ))
            .body(body.to_string())
            .send().await;

        match body {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let text = response.text().await;
                match text {
                    Ok(text) => Ok((text, status_code)),
                    Err(error) =>   Ok((error.to_string(), status_code))
                }
            },
            Err(err) =>{
                log::error!("Error during async put {}", err.to_string());
                Err((err.to_string(), 0))
            }
        }
    }

    
    pub async fn download_file_with_callback<F: Fn(&str, u64, u64, Arc<Mutex<DownloadStats>>)>(
        url: &Url,
        path: PathBuf,
        checksum: Option<String>,
        authorization: &String,
        stats: Arc<Mutex<DownloadStats>>,
        callback: F,
    ) -> Result<String, String> {
        let client = reqwest::Client::new();
        let res = client
            .get(url.as_str())
            .header(AUTHORIZATION, authorization)
            .send()
            .await
            .map_err(|_| format!("Failed to GET from '{}'", &url))?;
        if !res.status().is_success() {
            return Err(format!(
                "Failed to GET from '{}', Status: {}",
                &url,
                res.status()
            ));
        }
        let total_size = res
            .content_length()
            .ok_or(format!("Failed to get content length from '{}'", &url))?;
        let resume_support = match res.headers().get(ACCEPT_RANGES) {
            None => false,
            Some(value) => value.to_str().unwrap() == "bytes",
        };
        let mut file = match path.exists() && resume_support {
            true => { // We are attempting to complete a partial download and append it to the file
                OpenOptions::new()
                    .append(true)
                    .open(path.clone())
                    .map_err(|_| format!("Failed to open file '{}'", &path.display()))?
            }
            false => { // We are creating a new file to download the component into
                File::create(path.clone())
                    .map_err(|_| format!("Failed to create file '{}'", &path.display()))?
            }
        };
        let mut start_size = file.metadata().expect("Failed to get file data").len();
        if start_size >= total_size {
            // File already downloaded or overdownloaded! That means it's bad
            log::warn!(
                "Partially downloaded file equal or bigger than stated total size, overwriting..."
            );
            file = File::create(path.clone())
                .map_err(|_| format!("Failed to create file '{}'", &path.display()))?;
            start_size = 0;
        }
        log::info!(
            "Starting download from position {}/{} for file {}",
            start_size,
            total_size,
            path.to_string_lossy()
        );
        callback(path.to_str().unwrap(), start_size, total_size, stats.clone());
        let res = match resume_support {
            true => client
                .get(url.as_str())
                .header(AUTHORIZATION, authorization)
                .header(RANGE, format!("bytes={}-{}", start_size, (total_size - 1)))
                .send()
                .await
                .map_err(|_| format!("Failed to GET from '{}'", &url))?,

            false => res,
        };

        let mut downloaded: u64 = start_size;
        let mut stream = res.bytes_stream();
        let file_name = path.file_name().unwrap().to_str().unwrap();
        let mut last_time = (Instant::now().elapsed().as_millis() as f64) / 1000.0;
        last_time = if last_time > 60.0 { last_time - 60.0 } else { 0.0 };
        let mut last_percent: f32 = -100.0;
        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|_| "Error while downloading file".to_string())?;
            file.write_all(&chunk)
                .map_err(|_| "Error while writing to file".to_string())?;
            let new = min(downloaded + (chunk.len() as u64), total_size);
            downloaded = new;

            let percent: f32 = (new as f32 / total_size as f32) * 100_f32;
            let now = (Instant::now().elapsed().as_millis() as f64) / 1000.0;
            if percent == 100.0 || now - last_time > 3.0 || percent > last_percent + 5.0 {
                // Show message once in duration or percent threshold
                log::info!(
                    "Downloading {}: {:.2}% of {}",
                    file_name,
                    percent,
                    size_as_string(total_size)
                );
                last_time = now;
                last_percent = percent;
            }
            callback(path.to_str().unwrap(), downloaded, total_size, stats.clone());
        }

        if let Some(expected_checksum) = checksum {
            if downloaded >= total_size {   // We got the full file
                let actual_checksum = get_sha1_checksum(&path).unwrap_or_default();
                if actual_checksum != expected_checksum {   // Checksum is now what we expected
                    fs::remove_file(&path).await.map_err(|_| "Failed to delete file")?;
                    callback(path.to_str().unwrap(), 0, total_size, stats.clone()); // Reset progress bar back to 0
                    return Err(format!("Checksums don't match! Expected {} but got {}", expected_checksum, actual_checksum));
                }
            }
        }
        Ok(String::from("success"))
    }

    pub async fn report_eta(coupling_url: Url, token: &str, eta: u64){
        let token = Some(format!("Bearer {}", token));
        let coupling_url = coupling_url.join("api/v3/nodes/self/ota").unwrap();
        let json = serde_json::json!({"status": "downloading", "eta": eta});


       match RestServer::put(&coupling_url, token, &json).await {
           Ok((message, code)) => {log::debug!("Ok Put with message: {message} and code: {code}")}
           Err((message, code)) =>  {log::error!("Err Put with message: {message} and code: {code}")}
       }

    }
    fn construct_headers(authorization: Option<String>, content_type: ContentType) -> HeaderMap {
        let content_option = match content_type {
            ContentType::Protobuf => Some("application/octet-stream"),
            ContentType::Json => Some("application/json"),
            ContentType::Form => None,
        };

        let mut headers = HeaderMap::new();
        if let Some(token) = authorization {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&token).unwrap());
        }
        headers.insert(USER_AGENT, HeaderValue::from_static("reqwest"));
        if let Some(content_type) = content_option {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
        }
        headers
    }

    fn convert_to_storage_url(url: &Url) -> Result<Url, String> {
        let split_path = url.path().split('/');
        let mut split_vec: Vec<&str> = split_path.collect();
        split_vec.insert(2, "api/storage");
        let joined = split_vec.join("/");
        let url = &url[..Position::BeforePath];
        match Url::parse(url) {
            Ok(url) => match url.join(&joined) {
                Ok(url) => Ok(url),
                Err(error) => Err(format!("Faid joining URL {error}")),
            },
            Err(error) => Err(format!("Failed parsing URL {error}")),
        }
    }

    pub fn get_file_size_jfrog(url: &Url, authorization: Option<String>) -> Result<u64, String> {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct Response {
            size: String,
        }
        let api_url = RestServer::convert_to_storage_url(url)?;
        let (json_response, _) = RestServer::get(&api_url, authorization).map_err(|(message, _code)| message)?;
        let json: Result<Response, serde_json::Error> = serde_json::from_str(&json_response);
        match json {
            Ok(response) => match response.size.parse::<u64>() {
                Ok(size) => Ok(size),
                Err(error) => Err(format!(
                    "Error parsing file size: {}\n{}",
                    response.size, error
                )),
            },
            Err(error) => Err(format!(
                "Error while response deserialization {json_response}\n{error}",
            )),
        }
    }
    pub fn get_file_size(url: &Url, authorization: Option<String>) -> Result<u64, String> {
        let client = reqwest::blocking::Client::new();
        let response = client
            .get(url.as_str())
            .headers(RestServer::construct_headers(
                authorization,
                ContentType::Json,
            ))
            .header(RANGE, "bytes=0-0")
            .send();
        let response = RestServer::get_response(response).map_err(|(error, _code)| error)?;
        let (content_length_case, content_length_non_case) = (
            response.headers().get("Content-Range"),
            response.headers().get("content-range"),
        );

        let header_value_to_u64 = |header: &HeaderValue| -> Result<u64, String> {
            match header.to_str() {
                Ok(value) => {
                    // Get the number of bytes from: "bytes 0-0/325386240"
                    let size = match value.split('/').last() {
                        None => return Err("Unable to get size".to_string()),
                        Some(size) => size,
                    };

                    match size.parse() {
                        Ok(value) => Ok(value),
                        Err(error) => Err(error.to_string()),
                    }
                }
                Err(error) => Err(error.to_string()),
            }
        };

        match (content_length_case, content_length_non_case) {
            (Some(content_length), _) => Ok(header_value_to_u64(content_length)?),
            (_, Some(content_length)) => Ok(header_value_to_u64(content_length)?),
            (None, None) => Err("No Content-Length in the header".to_string()),
        }
    }

    pub fn post(
        url: &Url,
        body: &Value,
        authorization: Option<String>,
    ) -> Result<(String, u16), (String, u16)> {
        let json_string = body.to_string();
        log::trace!("POST url {}, body {}", url.as_str(), json_string);
        let client = reqwest::blocking::Client::new();
        let response = client
            .post(url.as_str())
            .headers(RestServer::construct_headers(
                authorization,
                ContentType::Json,
            ))
            .body(json_string)
            .send();
        RestServer::get_response_text(response)
    }

    pub fn put_json(
        url: &Url,
        body: &Value,
        authorization: Option<String>,
    ) -> Result<(String, u16), (String, u16)> {
        let json_string = body.to_string();
        log::trace!("PUT url {}, body {}", url.as_str(), json_string);
        let client = reqwest::blocking::Client::new();
        let response = client
            .put(url.as_str())
            .headers(RestServer::construct_headers(
                authorization,
                ContentType::Json,
            ))
            .body(json_string)
            .send();

        RestServer::get_response_text(response)
    }

    pub fn get_json(url: &Url, body: Option<&Value>, authorization: Option<String>) -> Result<(String, u16), (String, u16)> {
        let json_string = if let Some(json) = body { json.to_string() } else { "NONE".to_string() };
        log::trace!("url {}, body {}", url.as_str(), json_string);
        let client = reqwest::blocking::Client::new();
        let request = client
            .get(url.as_str())
            .headers(RestServer::construct_headers(
                authorization,
                ContentType::Json,
            ));
        let request = if body.is_none() { request } else { request.body(json_string) };
        let response = request.send();
        RestServer::get_response_text(response)
    }

    pub fn send_json(
        method: SendType,
        url: &Url,
        body: Option<&Value>,
        authorization: Option<String>,
    ) -> Result<(String, u16), (String, u16)> {
        match method {
            SendType::POST => RestServer::post(url, body.unwrap(), authorization),
            SendType::PUT => RestServer::put_json(url, body.unwrap(), authorization),
            SendType::FILE => {
                let file = PathBuf::from(body.unwrap()["file"].as_str().expect("Failed to parse payload!"));
                RestServer::send_file(url, &file, authorization)
            }
            SendType::GET => RestServer::get_json(url, body, authorization),
        }
    }

    pub fn send_file(
        url: &Url,
        file: &Path,
        authorization: Option<String>,
    ) -> Result<(String, u16), (String, u16)> {
        use reqwest::blocking::multipart::Form;

        let form = Form::new()
            .file("file", file.to_string_lossy().to_string())
            .expect("Failed to open file!");

        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let response = client
            .post(url.as_str())
            .headers(RestServer::construct_headers(
                authorization,
                ContentType::Form,
            ))
            .multipart(form)
            .send();

        let response = RestServer::get_response_text(response);

        match response.clone() {
            Ok(res) => {
                log::info!("Send file ok: {} {}", res.1, res.0);
            }
            Err((e, code)) => {
                log::error!("Send file error: {}, code {}", e, code);
            }
        }
        response
    }

    pub fn post_pf(
        url: &Url,
        body: &[u8],
        authorization: Option<String>,
    ) -> Result<String, String> {
        log::trace!("post_pf: url {}", url.as_str());
        let client = reqwest::blocking::Client::new();

        let response = client
            .post(url.as_str())
            .headers(RestServer::construct_headers(
                authorization,
                ContentType::Protobuf,
            ))
            .body(body.to_vec())
            .send();
        let response = RestServer::get_response_text(response);
        match response {
            Ok((content, _)) => Ok(content),
            Err((error, _)) => Err(error),
        }
    }

    fn get_response(
        call_response: Result<reqwest::blocking::Response, reqwest::Error>,
    ) -> Result<reqwest::blocking::Response, (String, u16)> {
        let response = match call_response {
            Ok(response) => response,
            Err(error) => return Err((format!("Error happened {error}"), 0)),
        };
        let response_code = response.status().as_u16();
        match response.status() {
            reqwest::StatusCode::OK
            | reqwest::StatusCode::PERMANENT_REDIRECT
            | reqwest::StatusCode::PARTIAL_CONTENT
            | reqwest::StatusCode::NO_CONTENT => Ok(response),
            reqwest::StatusCode::NOT_FOUND => {
                let url = response.url().clone();
                let text = response.text().unwrap();
                log::trace!("{url} responded with page not found - {response_code}, text: {text}");
                Err((format!(
                    "{} responded with page not found - {response_code}",
                    url.as_str()
                ), response_code))
            }
            other => {
                let text = response.text().unwrap();
                log::error!("Error occurred during REST call: {}", text);
                Err((format!("Responded with - {other}"), response_code))
            }
        }
    }

    fn get_response_text(
        body: Result<reqwest::blocking::Response, reqwest::Error>,
    ) -> Result<(String, u16), (String, u16)> {
        let response = RestServer::get_response(body)?;
        let status = response.status().as_u16();
        match response.text() {
            Ok(text) => Ok((text, status)),
            Err(err) => Err((format!(
                "Error occurred while get text from request body {err}"
            ), status))
        }
    }
}

#[cfg(test)]
pub fn extract_token(url: &Url, token: &str) -> String {
    let mut headers = reqwest::header::HeaderMap::new();
    let token = format!("Bearer {}", token);
    headers.insert("accept", "application/json".parse().unwrap());
    headers.insert("Authorization", token.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());

    let client = reqwest::blocking::Client::new();
    let body = {
        #[cfg(unix)] { "{\"checksums\":{\"core\":\"checksum\"},\"arch\":\"AMD64\"}" }
        #[cfg(windows)] { "{\"checksums\":{\"core\":\"checksum\"},\"arch\":\"WIN\"}" }
    };

    let address = url.join("/api/v3/versions/manifest").expect("Url fail").to_string();
    let res = client
        .post(address.clone())
        .headers(headers)
        .body(body)
        .send()
        .unwrap()
        .text()
        .unwrap();
    log::info!("URL IS {} Response is {}", address, res);
    let res: serde_json::Value = res.parse().unwrap();
    if res.is_object() {
        let obj = res.as_object().expect("Failed to parse object");
        let arr = obj["missingComponents"].as_array().expect("Failed to parse array");
        match arr[0]["token"].as_str() {
            None => { panic!("Failed to find token, response is {}", res); }
            Some(token) => { token.to_string() }
        }
    }
    else {
        match res[0]["token"].as_str() {
            None => { panic!("Failed to find token, response is {}", res); }
            Some(token) => { token.to_string() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn simple_test() {
        // Arrange
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/hi"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ohi"))
            .mount(&server)
            .await;

        //Act
        let url = Url::parse((server.uri() + "/hi").as_str()).unwrap();
        tokio::task::spawn_blocking(move || {
            let response = RestServer::get(&url, None);
            assert_eq!(response.unwrap(), ("ohi".to_string(), 200));
        })
            .await
            .ok();
    }

    #[tokio::test]
    async fn error_test() {
        // Arrange
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/error"))
            .respond_with(ResponseTemplate::new(404).set_body_string("FAIL"))
            .mount(&server)
            .await;

        //Act
        let url = Url::parse((server.uri() + "/error").as_str()).unwrap();
        tokio::task::spawn_blocking(move || {
            let response = RestServer::get(&url, None);
            // Assert
            assert!(response
                .unwrap_err().0
                .starts_with("Responded with page not found"));
        })
            .await
            .ok();
    }

    #[tokio::test]
    async fn post_test() {
        // Arrange
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/post"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let data =
            r#"{ "name": "John Doe", "age": 43, "phones": [ "+44 1234567", "+44 2345678" ] }"#;
        // Parse the string of data into serde_json::Value.
        let v: Value = serde_json::from_str(data).unwrap();

        //Act
        let url = Url::parse((server.uri() + "/post").as_str()).unwrap();
        tokio::task::spawn_blocking(move || {
            let response = RestServer::post(&url, &v, None);

            // Assert
            assert_eq!(response.unwrap(), ("".to_string(), 200));
        })
            .await
            .ok();
    }

    #[tokio::test]
    async fn post_pf_test() {
        // Arrange
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/post"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let v = vec![123, 10, 110];
        //Act
        let url = Url::parse((server.uri() + "/post").as_str()).unwrap();
        tokio::task::spawn_blocking(move || {
            let response = RestServer::post_pf(&url, &v, None);

            // Assert
            assert_eq!(response.unwrap(), "");
        })
            .await
            .ok();
    }

    #[tokio::test]
    async fn get_file_size() {
        // Arrange
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file_size"))
            .respond_with(
                ResponseTemplate::new(200).append_header("content-range", "bytes 0-0/821703"),
            )
            .mount(&server)
            .await;

        let url = Url::parse((server.uri() + "/file_size").as_str()).unwrap();

        tokio::task::spawn_blocking(move || {
            let response = RestServer::get_file_size(&url, None);
            assert_eq!(Ok(821703), response)
        })
            .await
            .ok();
    }

    #[tokio::test]
    async fn get_file_size_lower_case() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file_size"))
            .respond_with(ResponseTemplate::new(200).append_header("content-range", "821703"))
            .mount(&server)
            .await;

        let url = Url::parse((server.uri() + "/file_size").as_str()).unwrap();

        tokio::task::spawn_blocking(move || {
            let response = RestServer::get_file_size(&url, None);
            assert_eq!(Ok(821703), response)
        })
            .await
            .ok();
    }

    #[tokio::test]
    async fn get_file_size_lower_case_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file_size"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Authorization", "token")
                    .append_header("content-range", "821703"),
            )
            .mount(&server)
            .await;

        let url = Url::parse((server.uri() + "/file_size").as_str()).unwrap();

        tokio::task::spawn_blocking(move || {
            let response = RestServer::get_file_size(&url, Some("token".to_string()));
            assert_eq!(Ok(821703), response)
        })
            .await
            .ok();
    }

    #[tokio::test]
    async fn get_file_size_no_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file_size"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let url = Url::parse((server.uri() + "/file_size").as_str()).unwrap();
        tokio::task::spawn_blocking(move || {
            let response = RestServer::get_file_size(&url, None);
            assert!(response.is_err())
        })
            .await
            .ok();
    }

    #[test]
    #[ignore]
    fn get_file_size_jfrog_real() {
        let auth = Some("".to_string());
        let url = Url::parse("http://localhost:5000/core.snap").unwrap();
        let response = RestServer::get_file_size_jfrog(&url, auth).unwrap();
        assert!(response > 0);
    }

    #[test]
    #[ignore]
    fn get_file_size_s3() {
        let url = "https://phau-artifactory-eng2.s3.us-east-1.amazonaws.com/Phantom.Binary/Core/dev--DEV-9067-aws-upload/amd64/phau-core_0.0.1_amd64.snap?response-content-disposition=inline&X-Amz-Security-Token=IQoJb3JpZ2luX2VjEEkaDGV1LWNlbnRyYWwtMSJHMEUCIDgrjZnNwR6YDdVJ3OsoM0czwb9NFPVddgZOmxor9%2BotAiEAp2O4MKukb88O62yKHptgDuR%2FN2bynCfaOfeKeKT081kqgQMIQhABGgw5NTU3OTI3MDc4ODMiDEsEhRMoikoEbDhemCreAhEG%2BsBM%2Fdd4dx7g8z88Je%2Fxl3dFk3hGWwU9kYbyI983OORtk4wNr8Y2P1DUrlWYcL15d0xtZwViKGxJ6Ew4DkZxYsxMhqu4HlOoDQ8FfbEG89G0Rutwa07z8tjJ74yFDXg6u0L07%2FTeE0%2ByBk7XMqvNo1fujYZjjddzYgaqrvvmzXZVENJlyU%2FyRrJmRhqi5AfIX90sj%2BIT6e4fWe4DYjn1TR1f%2BrQ5OA7VWEIKbBiOhE7cFlXiDAybIVQt4%2Ftsy%2FeJa9UlCqAQeq6V64ltI52SNB%2BaSeRM6AznYY%2FEOwVyle%2FUZiL2LQp4gqyQTT5AZxDnnNhAu%2BrhipXl5rqAggcMX3N92bfhWwLJA9A%2F0u91o6QEQaURokrI%2BhbsQJzuPneymrx2OFqYHx16BSCik%2FRNde%2FldDCvNz1NcMKehtXDMC3m9GhBro0K0Rmge3wVY64i9oA%2B6Jx027cjnaMlMOm2qJsGOrMCdXX8SioZxomDr31O7EbOzmkt4IN9735OHknkMLvcT6bGNfN3PbW5g48PG4Eq9JbwKK37%2B2SloJeEoxKS27jvc%2B8whjhLXKwA3zhBzaN%2B6mufa5RZSV0LngzATkc0CEvqlQJQ6g35dHugWImmxiOjkN0%2FyuIlF2AeuyaLYVHDKb%2BDVMTX0f9tZErx7Uqpk3hfoIDqmKb8MYyVft6Pz7PRkZtvuTC1tJzExSIyG2SiNvcKEXY1PD064QyRBBHH2mTcu%2B0mrThkYG2rAUAx4GaIQxtFRziwoj%2FSxD3vXuROp6vb3hjHIDcOXQkBnCDSNaKOQm94Hj0wbk3D%2BCtGIKTb%2B5U0gvcmUU3ehJ%2BXfSSn88bag0cP1RONRHwPEZ27JoAwYQZXzu0l%2FA6b10t5FZGS4vCw4A%3D%3D&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Date=20221108T090426Z&X-Amz-SignedHeaders=host&X-Amz-Expires=43200&X-Amz-Credential=ASIA55CNPKEV6A3MASUH%2F20221108%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Signature=d49929efb4a29f1d9dc7313d73dedf3917de1eef7df7da2fbfb3d82c11ac21d6";
        let url = Url::parse(url).unwrap();
        let response = RestServer::get_file_size(&url, None).unwrap();
        assert!(response > 0);
    }

    #[test]
    #[ignore]
    fn get_file_size_jfrog() {
        let url = "https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK.Cpp/2.6.9/arm64/sdk-cpp-demo_2.6.9_arm64.snap";
        let token = "";
        let url = Url::parse(url).unwrap();
        let response = RestServer::get_file_size(&url, Some(token.to_string())).unwrap();
        assert!(response > 0);
    }

    #[test]
    fn convert_to_storage_url() {
        let url = Url::parse("https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/dev--DEV-10831--alexl/arm64/phantom-agent_0.6.2_arm64.snap").unwrap();
        let actual = RestServer::convert_to_storage_url(&url).unwrap();
        let expected = Url::parse("https://phantomauto.jfrog.io/artifactory/api/storage/Phantom.Binary/SDK-Phantom-Agent/dev--DEV-10831--alexl/arm64/phantom-agent_0.6.2_arm64.snap").unwrap();
        assert_eq!(actual, expected)
    }

    #[test]
    fn test_system_time() {
        use std::thread::sleep;
        use std::time::{Duration, SystemTime};

        let ins_time = Instant::now();
        let sys_time = SystemTime::now();
        sleep(Duration::from_millis(1));
        let new_ins_time = Instant::now();
        let new_sys_time = SystemTime::now();
        let ins_diff = ins_time.duration_since(new_ins_time);
        println!("Ins diff is {:?}", ins_diff);
        let result = std::panic::catch_unwind(|| sys_time.duration_since(new_sys_time).unwrap());
        assert!(result.is_err());
        println!("Sys diff panics");
    }


    #[tokio::test]
    #[ignore]
    async fn put_async() {
        let token = "Bearer eyJhbGciOiJQUzI1NiIsImtpZCI6IlpyZFBWZU10OU53TDZZVDlVcU9SZSJ9.eyJpc3MiOiJlbmcucGhhbnRvbWF1dG8uZGV2LyIsImF1ZCI6WyJlbmcucGhhbnRvbWF1dG8uZGV2L2NvdXBsaW5nIiwiZW5nLnBoYW50b21hdXRvLmRldi9wcm94eSJdLCJzdWIiOiI2NDJjMmZhNmNiMDIzYWU5ZTk0NjFmZjAiLCJleHAiOjE2ODExMTkxNjUsImlhdCI6MTY4MTAzMjc2NX0.T0vZnfsiMBECqhxTkdqDejXPsfJNLszSMf57HTXbJ23mwypkfRie3y1aCNqujmvsmcA-Cr5-xLv3ZI3tLjOQP94ifJk0Z0QdAU6SZyz4tkLwYt9_4n_P0SxIoGF7nkVBeAPolYBLIna_zx-kie2QeK0-fEKyVcnHFuvTS-_L1V42K9ScCppM4SFw601mV97p4d7riR2Ht53zHHrKLdHjuYHeYXupf3cqiERJtpffjttujg-Gx8rfmw_5LD00vtb5akorCOSoo7yJnhV1JPghrfgHm-_kfc4I36sMpLwNuxlhMo4HW4GkCMSp3hGBL7Xu1vvGP0C3axJSpdwteIDxeQ";
        let url = Url::parse("https://eng.phantomauto.dev/api/v3/nodes/self/ota").unwrap();
        let json = serde_json::json!({"status": "downloading", "eta": 5});

       match RestServer::put(&url, Some(token.to_string()), &json).await
       {
        Ok(response) => println!("{:?}", response),
        Err(err) => println!("{:?}", err)
       }
    }
}
