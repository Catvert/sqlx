use std::collections::HashMap;
use std::convert::TryInto;
use std::ops::Range;

use futures_core::future::BoxFuture;
use sha1::Sha1;

use crate::connection::{Connect, Connection};
use crate::executor::Executor;
use crate::mysql::protocol::{
    AuthPlugin, AuthSwitch, Capabilities, ComPing, Handshake, HandshakeResponse,
};
use crate::mysql::stream::MySqlStream;
use crate::mysql::util::xor_eq;
use crate::mysql::{rsa, tls};
use crate::url::Url;

// Size before a packet is split
pub(super) const MAX_PACKET_SIZE: u32 = 1024;

pub(super) const COLLATE_UTF8MB4_UNICODE_CI: u8 = 224;

/// An asynchronous connection to a [MySql] database.
///
/// The connection string expected by [Connection::open] should be a MySQL connection
/// string, as documented at
/// <https://dev.mysql.com/doc/refman/8.0/en/connecting-using-uri-or-key-value-pairs.html#connecting-using-uri>
///
/// ### TLS Support (requires `tls` feature)
/// This connection type supports some of the same flags as the `mysql` CLI application for SSL
/// connections, but they must be specified via the query segment of the connection string
/// rather than as program arguments.
///
/// The same options for `--ssl-mode` are supported as the `ssl-mode` query parameter:
/// <https://dev.mysql.com/doc/refman/8.0/en/connection-options.html#option_general_ssl-mode>
///
/// ```text
/// mysql://<user>[:<password>]@<host>[:<port>]/<database>[?ssl-mode=<ssl-mode>[&ssl-ca=<path>]]
/// ```
/// where
/// ```text
/// ssl-mode = DISABLED | PREFERRED | REQUIRED | VERIFY_CA | VERIFY_IDENTITY
/// path = percent (URL) encoded path on the local machine
/// ```
///
/// If the `tls` feature is not enabled, `ssl-mode=DISABLED` and `ssl-mode=PREFERRED` are no-ops and
/// `ssl-mode=REQUIRED`, `ssl-mode=VERIFY_CA` and `ssl-mode=VERIFY_IDENTITY` are forbidden
/// (attempting to connect with these will return an error).
///
/// If the `tls` feature is enabled, an upgrade to TLS is attempted on every connection by default
/// (equivalent to `ssl-mode=PREFERRED`). If the server does not support TLS (because `--ssl=0` was
/// passed to the server or an invalid certificate or key was used:
/// <https://dev.mysql.com/doc/refman/8.0/en/using-encrypted-connections.html>)
/// then it falls back to an unsecured connection and logs a warning.
///
/// Add `ssl-mode=REQUIRED` to your connection string to emit an error if the TLS upgrade fails.
///
/// However, like with `mysql` the server certificate is **not** checked for validity by default.
///
/// Specifying `ssl-mode=VERIFY_CA` will cause the TLS upgrade to verify the server's SSL
/// certificate against a local CA root certificate; this is not the system root certificate
/// but is instead expected to be specified as a local path with the `ssl-ca` query parameter
/// (percent-encoded so the URL remains valid).
///
/// If you're running MySQL locally it might look something like this (for `VERIFY_CA`):
/// ```text
/// mysql://root:password@localhost/my_database?ssl-mode=VERIFY_CA&ssl-ca=%2Fvar%2Flib%2Fmysql%2Fca.pem
/// ```
///
/// `%2F` is the percent-encoding for forward slash (`/`). In the example we give `/var/lib/mysql/ca.pem`
/// as the CA certificate path, which is generated by the MySQL server automatically if
/// no certificate is manually specified. Note that the path may vary based on the default `my.cnf`
/// packaged with MySQL for your Linux distribution. Also note that unlike MySQL, MariaDB does *not*
/// generate certificates automatically and they must always be passed in to enable TLS.
///
/// If `ssl-ca` is not specified or the file cannot be read, then an error is returned.
/// `ssl-ca` implies `ssl-mode=VERIFY_CA` so you only actually need to specify the former
/// but you may prefer having both to be more explicit.
///
/// If `ssl-mode=VERIFY_IDENTITY` is specified, in addition to checking the certificate as with
/// `ssl-mode=VERIFY_CA`, the hostname in the connection string will be verified
/// against the hostname in the server certificate, so they must be the same for the TLS
/// upgrade to succeed. `ssl-ca` must still be specified.
pub struct MySqlConnection {
    pub(super) stream: MySqlStream,
    pub(super) is_ready: bool,
    pub(super) cache_statement: HashMap<Box<str>, u32>,

    // Work buffer for the value ranges of the current row
    // This is used as the backing memory for each Row's value indexes
    pub(super) current_row_values: Vec<Option<Range<usize>>>,
}

fn to_asciz(s: &str) -> Vec<u8> {
    let mut z = String::with_capacity(s.len() + 1);
    z.push_str(s);
    z.push('\0');

    z.into_bytes()
}

async fn rsa_encrypt_with_nonce(
    stream: &mut MySqlStream,
    public_key_request_id: u8,
    password: &str,
    nonce: &[u8],
) -> crate::Result<Vec<u8>> {
    // https://mariadb.com/kb/en/caching_sha2_password-authentication-plugin/

    if stream.is_tls() {
        // If in a TLS stream, send the password directly in clear text
        return Ok(to_asciz(password));
    }

    // client sends a public key request
    stream.send(&[public_key_request_id][..], false).await?;

    // server sends a public key response
    let packet = stream.receive().await?;
    let rsa_pub_key = &packet[1..];

    // xor the password with the given nonce
    let mut pass = to_asciz(password);
    xor_eq(&mut pass, nonce);

    // client sends an RSA encrypted password
    rsa::encrypt::<Sha1>(rsa_pub_key, &pass)
}

async fn make_auth_response(
    stream: &mut MySqlStream,
    plugin: &AuthPlugin,
    password: &str,
    nonce: &[u8],
) -> crate::Result<Vec<u8>> {
    match plugin {
        AuthPlugin::CachingSha2Password | AuthPlugin::MySqlNativePassword => {
            Ok(plugin.scramble(password, nonce))
        }

        AuthPlugin::Sha256Password => rsa_encrypt_with_nonce(stream, 0x01, password, nonce).await,
    }
}

async fn establish(stream: &mut MySqlStream, url: &Url) -> crate::Result<()> {
    // https://dev.mysql.com/doc/dev/mysql-server/8.0.12/page_protocol_connection_phase.html
    // https://mariadb.com/kb/en/connection/

    // Read a [Handshake] packet. When connecting to the database server, this is immediately
    // received from the database server.

    let handshake = Handshake::read(stream.receive().await?)?;
    let mut auth_plugin = handshake.auth_plugin;
    let mut auth_plugin_data = handshake.auth_plugin_data;

    stream.capabilities &= handshake.server_capabilities;
    stream.capabilities |= Capabilities::PROTOCOL_41;

    log::trace!("using capability flags: {:?}", stream.capabilities);

    // Depending on the ssl-mode and capabilities we should upgrade
    // our connection to TLS

    tls::upgrade_if_needed(stream, url).await?;

    // Send a [HandshakeResponse] packet. This is returned in response to the [Handshake] packet
    // that is immediately received.

    let password = &*url.password().unwrap_or_default();
    let auth_response =
        make_auth_response(stream, &auth_plugin, password, &auth_plugin_data).await?;

    stream
        .send(
            HandshakeResponse {
                client_collation: COLLATE_UTF8MB4_UNICODE_CI,
                max_packet_size: MAX_PACKET_SIZE,
                username: url.username().unwrap_or("root"),
                database: url.database(),
                auth_plugin: &auth_plugin,
                auth_response: &auth_response,
            },
            false,
        )
        .await?;

    loop {
        // After sending the handshake response with our assumed auth method the server
        // will send OK, fail, or tell us to change auth methods
        let packet = stream.receive().await?;

        match packet[0] {
            // OK
            0x00 => {
                break;
            }

            // ERROR
            0xFF => {
                return stream.handle_err();
            }

            // AUTH_SWITCH
            0xFE => {
                let auth = AuthSwitch::read(packet)?;
                auth_plugin = auth.auth_plugin;
                auth_plugin_data = auth.auth_plugin_data;

                let auth_response =
                    make_auth_response(stream, &auth_plugin, password, &auth_plugin_data).await?;

                stream.send(&*auth_response, false).await?;
            }

            0x01 if auth_plugin == AuthPlugin::CachingSha2Password => {
                match packet[1] {
                    // AUTH_OK
                    0x03 => {}

                    // AUTH_CONTINUE
                    0x04 => {
                        // The specific password is _not_ cached on the server
                        // We need to send a normal RSA-encrypted password for this
                        let enc = rsa_encrypt_with_nonce(stream, 0x02, password, &auth_plugin_data)
                            .await?;

                        stream.send(&*enc, false).await?;
                    }

                    unk => {
                        return Err(protocol_err!("unexpected result from 'fast' authentication 0x{:x} when expecting OK (0x03) or CONTINUE (0x04)", unk).into());
                    }
                }
            }

            _ => {
                return stream.handle_unexpected();
            }
        }
    }

    Ok(())
}

async fn close(mut stream: MySqlStream) -> crate::Result<()> {
    // TODO: Actually tell MySQL that we're closing

    stream.flush().await?;
    stream.shutdown()?;

    Ok(())
}

async fn ping(stream: &mut MySqlStream) -> crate::Result<()> {
    stream.send(ComPing, true).await?;

    match stream.receive().await?[0] {
        0x00 | 0xFE => Ok(()),

        0xFF => stream.handle_err(),

        _ => stream.handle_unexpected(),
    }
}

impl MySqlConnection {
    pub(super) async fn new(url: crate::Result<Url>) -> crate::Result<Self> {
        let url = url?;
        let mut stream = MySqlStream::new(&url).await?;

        establish(&mut stream, &url).await?;

        let mut self_ = Self {
            stream,
            current_row_values: Vec::with_capacity(10),
            is_ready: true,
            cache_statement: HashMap::new(),
        };

        // After the connection is established, we initialize by configuring a few
        // connection parameters

        // https://mariadb.com/kb/en/sql-mode/

        // PIPES_AS_CONCAT - Allows using the pipe character (ASCII 124) as string concatenation operator.
        //                   This means that "A" || "B" can be used in place of CONCAT("A", "B").

        // NO_ENGINE_SUBSTITUTION - If not set, if the available storage engine specified by a CREATE TABLE is
        //                          not available, a warning is given and the default storage
        //                          engine is used instead.

        // NO_ZERO_DATE - Don't allow '0000-00-00'. This is invalid in Rust.

        // NO_ZERO_IN_DATE - Don't allow 'YYYY-00-00'. This is invalid in Rust.

        // --

        // Setting the time zone allows us to assume that the output
        // from a TIMESTAMP field is UTC

        // --

        // https://mathiasbynens.be/notes/mysql-utf8mb4

        self_.execute(r#"
SET sql_mode=(SELECT CONCAT(@@sql_mode, ',PIPES_AS_CONCAT,NO_ENGINE_SUBSTITUTION,NO_ZERO_DATE,NO_ZERO_IN_DATE'));
SET time_zone = '+00:00';
SET NAMES utf8mb4 COLLATE utf8mb4_unicode_ci;
        "#).await?;

        Ok(self_)
    }
}

impl Connect for MySqlConnection {
    fn connect<T>(url: T) -> BoxFuture<'static, crate::Result<MySqlConnection>>
    where
        T: TryInto<Url, Error = crate::Error>,
        Self: Sized,
    {
        Box::pin(MySqlConnection::new(url.try_into()))
    }
}

impl Connection for MySqlConnection {
    fn close(self) -> BoxFuture<'static, crate::Result<()>> {
        Box::pin(close(self.stream))
    }

    fn ping(&mut self) -> BoxFuture<crate::Result<()>> {
        Box::pin(ping(&mut self.stream))
    }
}
