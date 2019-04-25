extern crate chrono;
extern crate webpki;
extern crate rustls;
extern crate base64;
extern crate itertools;
extern crate untrusted;
extern crate serde_json;
extern crate num_bigint;
extern crate bit_vec;
extern crate hex;
extern crate sgx_types;

use sgx_types::*;

use std::fs;
use std::fs::File;
use std::io::{self, Write, Read, BufReader};
use std::io::prelude::*;
use std::sync::Arc;
use std::str;
use std::net::TcpStream;

mod cert;
mod pib;

const SERVERADDR: &str = "localhost:3443";
const VERIFYMSADDR: &str = "localhost:3444";
const MSFILE: &str = "./measurement.txt";

struct ServerAuth {
    outdated_ok: bool
}

impl ServerAuth {
    fn new(outdated_ok: bool) -> ServerAuth {
        ServerAuth{ outdated_ok }
    }
}

impl rustls::ServerCertVerifier for ServerAuth {
    fn verify_server_cert(&self,
              _roots: &rustls::RootCertStore,
              _certs: &[rustls::Certificate],
              _hostname: webpki::DNSNameRef,
              _ocsp: &[u8]) -> Result<rustls::ServerCertVerified, rustls::TLSError> {
        println!("--received-server cert: {:?}", _certs);
        // This call will automatically verify cert is properly signed
        match cert::verify_mra_cert(&_certs[0].0) {
            Ok(()) => {
                Ok(rustls::ServerCertVerified::assertion())
            }
            Err(sgx_status_t::SGX_ERROR_UPDATE_NEEDED) => {
                if self.outdated_ok {
                    println!("outdated_ok is set, overriding outdated error");
                    Ok(rustls::ServerCertVerified::assertion())
                } else {
                    Err(rustls::TLSError::WebPKIError(webpki::Error::ExtensionValueInvalid))
                }
            }
            Err(_) => {
                Err(rustls::TLSError::WebPKIError(webpki::Error::ExtensionValueInvalid))
            }
        }
    }
}

fn make_config() -> rustls::ClientConfig {
    let mut config = rustls::ClientConfig::new();

    let client_cert = include_bytes!("../../cert/client.crt");
    let mut cc_reader = BufReader::new(&client_cert[..]);

    let client_pkcs8_key = include_bytes!("../../cert/client.pkcs8");
    let mut client_key_reader = BufReader::new(&client_pkcs8_key[..]);

    let certs = rustls::internal::pemfile::certs(&mut cc_reader).unwrap();
    let privk = rustls::internal::pemfile::pkcs8_private_keys(&mut client_key_reader);

    config.set_single_client_cert(certs, privk.unwrap()[0].clone());

    config.dangerous().set_certificate_verifier(Arc::new(ServerAuth::new(true)));
    config.versions.clear();
    config.versions.push(rustls::ProtocolVersion::TLSv1_2);

    config
}

fn main() {
    println!("Starting tr-mpc-client");

    println!("Connecting to verify server: {}", VERIFYMSADDR);

    let mut result =  fs::remove_file(MSFILE);

    let client_config = make_config();
    let dns_name = webpki::DNSNameRef::try_from_ascii_str("localhost").unwrap();
    let mut sess = rustls::ClientSession::new(&Arc::new(client_config), dns_name);

    let mut conn = TcpStream::connect(VERIFYMSADDR).unwrap();

    let mut tls = rustls::Stream::new(&mut sess, &mut conn);

    tls.write_all(b"hello").unwrap();

    let mut plaintext = Vec::new();
    match tls.read_to_end(&mut plaintext) {
        Ok(_) => {
            println!("Server replied: {}", str::from_utf8(&plaintext).unwrap());
        }
        Err(ref err) if err.kind() == io::ErrorKind::ConnectionAborted => {
            println!("EOF (tls)");
        }
        Err(e) => println!("Error in read_to_end: {:?}", e),
    }


    println!("Connecting to server: {}", SERVERADDR);

    let client_config = make_config();
    let dns_name = webpki::DNSNameRef::try_from_ascii_str("localhost").unwrap();
    let mut sess = rustls::ClientSession::new(&Arc::new(client_config), dns_name);

    let mut conn = TcpStream::connect(SERVERADDR).unwrap();

    let mut tls = rustls::Stream::new(&mut sess, &mut conn);

    //we are trying to vefity the measurement of remote server
    let file = File::open(MSFILE).unwrap();
    let mut fin = BufReader::new(file);
    let mut i = 0;

    let mut ms_str_server = String::new();
    let mut ms_str_verifyserver = String::new();

    for line in fin.lines() {
        if i==0 {
            ms_str_server = line.unwrap()
        }else{
            ms_str_verifyserver = line.unwrap()
        }
        i = i+1;
    }

    if ms_str_server == ms_str_verifyserver {
        println!("verify successd");
    }else{
        println!("failed to verify ms");
        return
    }

    tls.write_all(b"hello").unwrap();

    let mut plaintext = Vec::new();
    match tls.read_to_end(&mut plaintext) {
        Ok(_) => {
            println!("Server replied: {}", str::from_utf8(&plaintext).unwrap());
        }
        Err(ref err) if err.kind() == io::ErrorKind::ConnectionAborted => {
            println!("EOF (tls)");
        }
        Err(e) => println!("Error in read_to_end: {:?}", e),
    }
}
