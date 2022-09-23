mod request;
mod response;

use std::{collections::HashMap, sync::Arc};

use clap::Parser;
use http::StatusCode;
use rand::{Rng, SeedableRng};
use std::io::ErrorKind;
use tokio::{
    net::{TcpListener, TcpStream},
    stream::StreamExt,
    sync::RwLock,
    time::{self, Duration},
};

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Parser, Debug)]
#[clap(about = "Fun with load balancing")]
struct CmdOptions {
    #[clap(
        short,
        long,
        help = "IP/port to bind to",
        default_value = "0.0.0.0:1100"
    )]
    bind: String,
    #[clap(short, long, help = "Upstream host to forward requests to")]
    upstream: Vec<String>,
    #[clap(
        long,
        help = "Perform active health checks on this interval (in seconds)",
        default_value = "10"
    )]
    active_health_check_interval: usize,
    #[clap(
        long,
        help = "Path to send request to for active health checks",
        default_value = "/"
    )]
    active_health_check_path: String,
    #[clap(
        long,
        help = "Maximum number of requests to accept per IP per minute (0 = unlimited)",
        default_value = "0"
    )]
    max_requests_per_minute: usize,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    active_health_check_path: String,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    max_requests_per_minute: usize,
    /// Addresses of servers that we are proxying to
    upstream_addresses: Vec<String>,
    /// Whether the upstream address is dead
    upstream_dead: RwLock<Vec<bool>>,
    /// The rate limiter tracks counters for each IP
    requests_counters: RwLock<HashMap<String, usize>>,
}

#[tokio::main]
async fn main() {
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.len() < 1 {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let mut listener = match TcpListener::bind(&options.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            log::error!("Could not bind to {}: {}", options.bind, err);
            std::process::exit(1);
        }
    };
    log::info!("Listening for requests on {}", options.bind);

    let upstream_dead = vec![false; options.upstream.len()];
    let state = Arc::new(ProxyState {
        upstream_addresses: options.upstream,
        upstream_dead: RwLock::new(upstream_dead),
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        max_requests_per_minute: options.max_requests_per_minute,
        requests_counters: RwLock::new(HashMap::new()),
    });

    tokio::spawn(active_health_check(Arc::clone(&state)));

    if state.max_requests_per_minute != 0 {
        tokio::spawn(reset_counters(Arc::clone(&state)));
    }

    // Handle incoming connections
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        if let Ok(stream) = stream {
            let state = Arc::clone(&state);
            // Handle the connection!
            tokio::spawn(handle_connection(stream, state));
        }
    }
}

async fn active_health_check_upstream(
    state: Arc<ProxyState>,
    upstream_address: &String,
) -> Option<()> {
    let mut stream = TcpStream::connect(upstream_address).await.ok()?;
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(&state.active_health_check_path)
        .header("Host", upstream_address)
        .body(Vec::new())
        .unwrap();
    request::write_to_stream(&request, &mut stream).await.ok()?;

    let response = response::read_from_stream(&mut stream, request.method())
        .await
        .ok()?;
    if response.status() == StatusCode::OK {
        Some(())
    } else {
        None
    }
}

async fn reset_counters(state: Arc<ProxyState>) {
    let mut interval = time::interval(Duration::from_secs(60));
    interval.tick().await;
    loop {
        interval.tick().await;
        state.requests_counters.write().await.clear();
    }
}

async fn active_health_check(state: Arc<ProxyState>) {
    let mut interval = time::interval(Duration::from_secs(
        state.active_health_check_interval as u64,
    ));
    interval.tick().await;
    loop {
        interval.tick().await;
        // health check here
        for (index, upstream_address) in state.upstream_addresses.iter().enumerate() {
            state.upstream_dead.write().await[index] =
                active_health_check_upstream(Arc::clone(&state), upstream_address)
                    .await
                    .is_none();
        }
    }
}

async fn connect_to_upstream(state: Arc<ProxyState>) -> Result<TcpStream, std::io::Error> {
    let mut rng = rand::rngs::StdRng::from_entropy();
    loop {
        let alive_idxes: Vec<usize> = state
            .upstream_dead
            .read()
            .await
            .iter()
            .enumerate()
            .filter_map(|(index, &dead)| if dead { None } else { Some(index) })
            .collect();
        if alive_idxes.is_empty() {
            return Err(std::io::Error::from(ErrorKind::ConnectionRefused));
        }

        let random_idx = alive_idxes[rng.gen_range(0, alive_idxes.len())];
        let upstream_ip = &state.upstream_addresses[random_idx];

        match TcpStream::connect(upstream_ip).await {
            Ok(stream) => break Ok(stream),
            Err(_) => state.upstream_dead.write().await[random_idx] = true,
        }
    }
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!(
        "{} <- {}",
        client_ip,
        response::format_response_line(&response)
    );
    if let Err(error) = response::write_to_stream(&response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
        return;
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: Arc<ProxyState>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    if state.max_requests_per_minute != 0
        && *state
            .requests_counters
            .write()
            .await
            .entry(client_ip.clone())
            .and_modify(|counts| *counts += 1)
            .or_insert(1)
            > state.max_requests_per_minute
    {
        let response = response::make_http_error(http::StatusCode::TOO_MANY_REQUESTS);
        send_response(&mut client_conn, &response).await;
        return;
    }

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = upstream_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => request,
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!(
                "Failed to send request to upstream {}: {}",
                upstream_ip,
                error
            );
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await
        {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}
