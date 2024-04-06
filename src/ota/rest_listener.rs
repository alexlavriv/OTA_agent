use std::{
    borrow::BorrowMut,
    collections::HashMap,
    convert::Infallible,
    mem::MaybeUninit,
    net::SocketAddr,
    sync::mpsc,
    sync::{Mutex, Once},
    thread,
};

use crate::{OTAStatus, OTAStatusRestResponse, RestMessage};
use hyper::{service::{make_service_fn, service_fn}, Body, Request, Response, Server, Uri};

use spdlog::info;

type CallbacksContainer = Mutex<
    HashMap<
        String,
        (
            Option<(mpsc::Sender<RestMessage>, RestMessage)>,
            fn(Uri, String) -> Result<String, String>,
        ),
    >,
>;

pub struct RestListener {
    port: u16,
    callbacks: CallbacksContainer,
    ota_status: Mutex<OTAStatusRestResponse>,
}

impl RestListener {
    fn new(port: u16) -> Self {
        log::info!("Creating rest server on port {}", port);
        Self {
            port,
            callbacks: Mutex::new(HashMap::new()),
            ota_status: Mutex::new(OTAStatusRestResponse{
                ota_status: OTAStatus::ERROR,
                message: "".to_string(),
                manifest_version: "".to_string()
            } ),
        }
    }

    pub fn unstring( str: String ) -> String {
        let str = str.replace("\\\"", "\""); // Client compatibility?
        str[1..str.len()-1].to_string()
    }
    pub fn log(uri: &Uri, message: &str){
        if *uri == "/status" {
            log::trace!("{}", message);
        }
        else{
            log::info!("{}", message);
        }
    }
    async fn response_function(request: Request<Body>) -> Result<Response<Body>, Infallible> {
        use std::ops::Deref;
        let uri = request.uri().clone();
        Self::log(&uri, format!("Got request: {:?}", request).as_str());
        let body_bytes = hyper::body::to_bytes(request.into_body()).await.unwrap();
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();
        let body_str = if !body_str.is_empty() && body_str.starts_with('\"') {
            Self::unstring(body_str)
        } else { body_str };

        let parts = uri.path().split('/').collect::<Vec<&str>>();
        let key = if parts.len() > 1 { parts[1] } else { "" };

        Self::log(&uri, format!("URI: {:?}, BODY: [{}], KEY: [{}]", uri.to_string(), body_str, key).as_str());

        let (status, body) = {
            let map = match rest_listener().callbacks.lock() {

                Ok(inner_map) => { inner_map.deref().clone() }
                Err(e) => {
                    e.get_ref().deref().clone()
                }
            };
            if map.contains_key(key) {
                let (trigger, callback) = map.get(key).unwrap();
                let (code, response) = match callback(uri.clone(), body_str) {
                    Ok(response) => { (200, response) }
                    Err(response) => { (500, response) }
                };
                match trigger {
                    None => { (code, response) } // Non-channel callback returns response immediately
                    Some((sender_channel, message)) => {
                        log::info!("Sending {:?} trigger via channel", message);
                        match sender_channel.send(message.clone()) {
                            Ok(_) => (code, response),
                            Err(e) => {
                                log::error!("Rest Listener could not send request: {}", e);
                                (503, "SEND FAIL\n".to_string())
                            }
                        }
                    }
                }
            } else {
                (501, "NO SUCH CALLBACK\n".to_string())
            }
        };
  
        Self::log(&uri, &format!("RESPONDING WITH STATUS {} BODY {}", status, body));


        Ok(Response::builder()
            .status(status).header("Access-Control-Allow-Origin", "*")
            .body(Body::from(body))
            .unwrap())
    }

    #[tokio::main]
    async fn serving(port: u16) {
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let make_service = make_service_fn(|_conn| async {
            Ok::<_, Infallible>(service_fn(RestListener::response_function))
        });

        let server = Server::bind(&addr).serve(make_service);
        if let Err(e) = server.await {
            log::error!("Server error: {}", e);
        }
    }

    #[tokio::main]
    async fn run(&'static self) {
        thread::Builder::new()
            .name("REST listener".to_string())
            .spawn(move || {
                log::info!("REST Listener singleton thread is running");
                RestListener::serving(self.port);
                log::info!("REST Listener singleton thread finished running"); // Probably unreachable
            })
            .expect("Could not spawn REST Listener thread");
    }

    pub fn add_callback(
        &self,
        request: String,
        trigger: Option<(mpsc::Sender<RestMessage>, RestMessage)>,
        callback: fn(Uri, String) -> Result<String, String>,
    ) {
        match trigger.clone() {
            None => { info!("Adding the following callback: {} (no trigger)", request); }
            Some((_, message)) => { info!("Adding the following callback: {} (triggers {:?})", request, message); }
        }

        self.callbacks
            .lock()
            .unwrap()
            .borrow_mut()
            .insert(request, (trigger, callback));
    }
}

pub fn rest_listener() -> &'static RestListener {
    create_rest_listener(None)
}

pub fn create_rest_listener(port: Option<u16>) -> &'static RestListener {
    static mut SINGLETON: MaybeUninit<RestListener> = MaybeUninit::uninit();
    static ONCE: Once = Once::new();
    unsafe {
        if ONCE.is_completed() && port.is_some() {
            log::error!("Attempting to recreate rest listener, but it's already running!");
        }
        ONCE.call_once(|| {
            let singleton = RestListener::new(port.expect("Must provide port!"));
            SINGLETON.write(singleton);
            SINGLETON.assume_init_ref().run();
        });
        SINGLETON.assume_init_ref()
    }
}

pub fn set_ota_status(status: OTAStatusRestResponse) {
    let mut my_status = rest_listener().ota_status.lock().unwrap();
    *my_status = status;
}

pub fn get_ota_status() -> OTAStatusRestResponse {
    rest_listener().ota_status.lock().unwrap().clone()
}

#[cfg(test)]
mod test {
    use crate::ota::rest_listener::rest_listener;
    use crate::ota::rest_listener::{create_rest_listener, RestListener};
    use crate::utils::log_utils::set_logging_for_tests;
    use crate::{get_ota_status, OTAManager, OTAStatus, OTAStatusRestResponse, RestMessage, set_ota_status};
    use crate::RestMessage::UpdateVersion;
    use std::io::{stdout, Write};
    use std::time::Duration;
    use hyper::Uri;

    fn post_callback(uri: Uri, data: String) -> Result<String, String> {
        log::info!("POST CALLBACK: Uri is {} Data is [{}]", uri.to_string(), data);
        Ok(format!("RESPONSE TO {}", uri.to_string()))
    }

    fn send_custom_log_test(uri: Uri, _: String) -> Result<String, String> {
        let parts = uri.path().split('/').collect::<Vec<&str>>();
        if parts.len() < 2 || parts[1].to_lowercase() != "log" {
            log::warn!("Custom log requested, but {} doesn't match expected format", uri);
            return Err("Unexpected URI format".to_string());
        }
        if parts.len() == 2 || parts[2].is_empty() {
            return Ok(format!("Log sent to DEFAULT"));
        }
        let ticket = parts[2].to_uppercase();
        let ticket = match ticket.starts_with("DEV-") {
            true => {&ticket[4..]}
            false => {&ticket}
        };
        let mut valid = !ticket.is_empty();
        for c in ticket.chars() {
            if !c.is_numeric() {
                valid = false;
            }
        }
        if !valid {
            log::warn!("Custom log requested, but ticket [{}] is invalid", ticket);
            return Err("Invalid ticket".to_string());
        }
        let ticket = "DEV-".to_owned() + ticket;
        Ok(format!("Log sent to {}", ticket))
    }

    #[test]
    #[ignore]
    fn callback_rest_listener_test() {
        // Run the test with --nocapture, then browse to localhost:33333/test1 localhost:33333/test2 to see results
        set_logging_for_tests(log::LevelFilter::Info);
        use std::sync::mpsc;
        let (sender1, receiver1) = mpsc::channel::<RestMessage>();
        create_rest_listener(Some(33333));
  /*      rest_listener().add_callback(
            "test1".to_string(),
            Some((sender1, UpdateVersion)),
            |_,_| { Ok("Phantom Agent is checking for updates, run\n\npowershell -command \"Get-Content 'C:\\Program Files\\phantom_agent\\log\\phantom_agent.log' -Wait -Tail 30\n\nto follow the progress.\n".to_string()) },
        );*/
        rest_listener().add_callback(
            "test2".to_string(),
            None,
            post_callback
        );
        rest_listener().add_callback(
            "log".to_string(),
            None,
            send_custom_log_test
        );
        rest_listener().add_callback(
            "check".to_string(),
            None,
            OTAManager::<crate::SystemCtl>::check_versions
        );
        rest_listener().add_callback(
            "update_version".to_string(),
            Some((sender1, UpdateVersion)),
            OTAManager::<crate::SystemCtl>::update_version,
        );
        rest_listener().add_callback(
            "write_to_log".to_string(),
            None,
            OTAManager::<crate::SystemCtl>::write_to_log,
        );

        let mut count = 0;
        while count < 1000 {
            count += 1;
            print!(".");
            stdout().flush().unwrap();
            if let Ok(x) = receiver1.recv_timeout(Duration::new(1, 0)) {
                println!("TICK {}: Receiver 1 got {:?}", count, x);
            }
        }
    }

    #[tokio::main]
    async fn hyper_serve(port: u16) {
        use hyper::{
            service::{make_service_fn, service_fn},
            Server,
        };
        use std::{convert::Infallible, net::SocketAddr};

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let make_service = make_service_fn(|_conn| async {
            Ok::<_, Infallible>(service_fn(RestListener::response_function))
        });
        let server = Server::bind(&addr).serve(make_service);
        if let Err(e) = server.await {
            log::error!("server error: {}", e);
        }
    }

    #[test]
    #[ignore]
    fn hyper_listener_test() {
        set_logging_for_tests(log::LevelFilter::Info);
        hyper_serve(33334);
    }

    #[test]
    #[ignore]
    fn status_rest_listener_test() {
        // Run the test with --nocapture, then browse to localhost:33333/test1 localhost:33333/test2 to see results
        set_logging_for_tests(log::LevelFilter::Info);
        create_rest_listener(Some(33333));
        let status = get_ota_status();
        log::info!("STATUS IS {:?}", status);
        let new_status = OTAStatusRestResponse {
            ota_status: OTAStatus::CHECKING,
            message: "Message1".to_string(),
            manifest_version: "Version1".to_string(),
        };
        set_ota_status(new_status);
        let status = get_ota_status();
        log::info!("STATUS IS {:?}", status);
        let new_status = OTAStatusRestResponse {
            ota_status: OTAStatus::UPDATED,
            message: "Message2".to_string(),
            manifest_version: "Version2".to_string(),
        };
        set_ota_status(new_status);
        let status = get_ota_status();
        log::info!("STATUS IS {:?}", status);
    }
}
