// dropbox_api_token_with_oauth2_mod.rs

// TODO: Change to be UI agnostic

//! # decrypt dropbox api token from file or use the oauth2 "PKCE code workflow" to get the access token and encrypt it and save into file
//!
//! Dropbox offers the preferred OAuth2 "PKCE with code flow" for client applications.
//! In clients that run on the client machine it is impossible to hide secrets.
//! So there is the new recommended PKCE "pixie" (Proof Key for Code Exchange) to the rescue.
//! <https://dropbox.tech/developers/pkce--what-and-why->
//!
//! ## Secrets
//!
//! In this module there will be a lot of work with secrets.  
//! It is difficult to trust an external crate with your secrets.  
//! External crates can get updated unexpectedly and change to malicious code.  
//!
//! ## Copy code instead of dependency crate
//!
//! It is best to have the Rust code under your fingertips when dealing with secrets.  
//! Than you know, nobody will touch this code except of you.  
//! You can copy this code directly into your codebase as a module,
//! inspect and review it and know exactly what is going on.  
//! The code is as linear and readable with comments as possible.
//!
//! ## Store encrypted secret to file
//!
//! The secrets will be encrypted with an ssh private key and stored in the `~/.ssh` folder.  
//! This way the data is protected at rest in storage drive.  
//!
//! ## In memory protection
//!
//! This is a tough one! There is no 100% software protection of secrets in memory.  
//! Theoretically an attacker could dump the memory in any moment and read the secrets.  
//! There is always a moment when the secret is used in its plaintext form. This cannot be avoided.
//! All we can do now is to be alert what data is secret and take better care of it.  
//! Every variable that have secrets will have the word `secret` in it.
//! When a variable is confusing I will use the word `plain` to express it is `not a secret`.
//! To avoid leaking in logs I will use the `secrecy` crate. This is not 100% protection. It is important just to express intent when the secrets are really used.  
//! `Secrecy` needs the trait `zeroize` to empty the memory after use for better memory hygiene.
//! I will add the type names explicitly to emphasis the secrecy types used.
//!
//! ## encrypt_decrypt_with_ssh_key_mod
//!
//! This module depends on the generic module for encryption `encrypt_decrypt_with_ssh_key_mod.rs`. That module also needs to be copy and paste into your project.
//!
//! ## Other dependencies
//!
//! In `Cargo.toml` there are a group od dependencies needed for this to work. They are so generic that I don't expect any malware in them to be able to steal some usable secrets.  
//!
//! Beware that the versions of crates in `Cargo.toml` are not precisely pinpointed. In rust the symbol '=' means "the same major number equal or newer to". This means from one compilation to another, it can automatically change to a newer version without the programmer even noticing it.
//!
//! This is great if the newer version is solving some security issue. But this is super-bad if the newer version is malware supply chain attack. We have no idea how to distinguish one from another.
//!
//! Just to mention: there exists the trick to control the `Cargo.lock` file and forbid the change of the version number, but more times than not, you will not want to commit the lock file into the Dropbox repository.
//!
//! ```toml
//! [dependencies]
//! reqwest={version="0.12.12", features=["json","blocking"]}
//! serde ={ version= "1.0.217", features=["std","derive"]}
//! serde_json = "1.0.138"
//! ssh-key = { version = "0.6.7", features = [ "rsa", "encryption","ed25519"] }
//! ssh_agent_client_rs_git_bash = "0.0.19"
//! rsa = { version = "0.9.7", features = ["sha2","pem"] }
//! zeroize = {version="1.8.1", features=["derive"]}
//! aes-gcm = "0.10.3"
//! base64ct = {version = "1.6.0", features = ["alloc"] }
//! secrecy = "0.10.3"
//! chrono ="0.4.39"
//! crossplatform_path="1.1.1"
//! ```
//!

#![allow(dead_code)]

use rsa::sha2::Digest;
use secrecy::{ExposeSecret, SecretBox, SecretString};

use crate::{Error, Result};

use super::encrypt_decrypt_mod as ende;

// region: Public API constants
// ANSI colors for Linux terminal
// https://github.com/shiena/ansicolor/blob/master/README.md
/// ANSI color
pub const RED: &str = "\x1b[31m";
/// ANSI color
pub const GREEN: &str = "\x1b[32m";
/// ANSI color
pub const YELLOW: &str = "\x1b[33m";
/// ANSI color
pub const BLUE: &str = "\x1b[34m";
/// ANSI color
pub const RESET: &str = "\x1b[0m";
// endregion: Public API constants

#[derive(serde::Deserialize, serde::Serialize)]
pub struct DropboxApiConfig {
    pub dropbox_app_name: String,
    /// AKA app_name
    pub client_id: String,
    pub dropbox_api_private_key_file_name: String,
}

/// Application state (static) is initialized only once in the main() function.
///
/// And then is accessible all over the code.
pub static DROPBOX_API_CONFIG: std::sync::OnceLock<DropboxApiConfig> = std::sync::OnceLock::new();

#[derive(serde::Deserialize, serde::Serialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop, Debug)]
struct SecretResponseAccessToken {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
}

/// Application state (static) is initialized only once in the main() function.
///
/// And then is accessible all over the code.
pub fn dropbox_api_config_initialize() {
    if DROPBOX_API_CONFIG.get().is_some() {
        return;
    }

    let dropbox_api_config_json = std::fs::read_to_string("dropbox_api_config.json")
        .unwrap_or_else(|_| panic!("{RED}Error: The file dropbox_api_config.json is missing.{RESET}"));
    let dropbox_api_config: DropboxApiConfig = serde_json::from_str(&dropbox_api_config_json)
        .unwrap_or_else(|_| panic!("{RED}Error: The content of dropbox_api_config.json is not correct.{RESET}"));
    let _ = DROPBOX_API_CONFIG.set(dropbox_api_config);
}

/// Start the dropbox oauth2 PKCE code workflow
/// It will use the private key from the .ssh folder.
/// The encrypted file has the same file name with the ".enc" extension.
/// Returns access_token to use as bearer for api calls
pub fn get_dropbox_secret_token(client_id: &str) -> Result<SecretString> {
    let private_key_file_name = DROPBOX_API_CONFIG
        .get()
        .ok_or_else(|| Error::ErrorFromStr("DROPBOX_API_CONFIG.get error"))?
        .dropbox_api_private_key_file_name
        .to_string();
    println!("  {YELLOW}Check if the ssh private key exists.{RESET}");
    let private_key_path_struct = ende::PathStructInSshFolder::new(private_key_file_name.clone())?;
    if !private_key_path_struct.exists() {
        println!("{RED}Error: Private key {private_key_path_struct} does not exist.{RESET}");
        println!("  {YELLOW}Create the private key in bash terminal:{RESET}");
        println!(r#"{GREEN}ssh-keygen -t ed25519 -f {private_key_path_struct} -C "dropbox api secret_token"{RESET}"#);
        return Err(Error::ErrorFromStr("Private key file not found."));
    }

    println!("  {YELLOW}Check if the encrypted file exists.{RESET}");
    let encrypted_path_struct = ende::PathStructInSshFolder::new(format!("{private_key_file_name}.enc"))?;
    if !encrypted_path_struct.exists() {
        println!("  {YELLOW}Encrypted file {encrypted_path_struct} does not exist.{RESET}");
        println!("  {YELLOW}Continue to authentication with the browser{RESET}");
        let secret_access_token = authenticate_with_browser_and_save_file(client_id, &private_key_path_struct, &encrypted_path_struct)?;
        Ok(secret_access_token)
    } else {
        println!("  {YELLOW}Encrypted file {encrypted_path_struct} exist.{RESET}");
        let plain_file_text = ende::open_file_b64_get_string(encrypted_path_struct.get_cross_path())?;
        // deserialize json into struct
        let encrypted_text_with_metadata: ende::EncryptedTextWithMetadata = serde_json::from_str(&plain_file_text)?;

        // check the expiration
        let utc_now = chrono::Utc::now();

        if encrypted_text_with_metadata.access_token_expiration.is_none() {
            return Err(Error::ErrorFromStr("access_token_expiration is None"));
        }

        let access_token_expiration = chrono::DateTime::parse_from_rfc3339(
            encrypted_text_with_metadata
                .access_token_expiration
                .as_ref()
                .expect("The former line asserts this is never None"),
        )?;
        if access_token_expiration <= utc_now {
            println!("{RED}Access token has expired, use refresh token{RESET}");
            let secret_decrypted_from_file = decrypt_text_with_metadata(encrypted_text_with_metadata)?;
            let secret_response_access_token: SecretBox<SecretResponseAccessToken> = refresh_tokens(
                client_id,
                secret_decrypted_from_file
                    .expose_secret()
                    .refresh_token
                    .clone()
                    .ok_or_else(|| Error::ErrorFromStr("refresh_token is None"))?,
            )?;

            let secret_access_token = SecretString::from(secret_response_access_token.expose_secret().access_token.to_string());
            let expires_in = secret_response_access_token.expose_secret().expires_in;

            // create new secret_response_access_token, because the response of refresh does not have the refresh_token
            let secret_response_access_token = SecretResponseAccessToken {
                access_token: secret_access_token.expose_secret().to_string(),
                expires_in,
                refresh_token: secret_decrypted_from_file.expose_secret().refresh_token.clone(),
            };
            let secret_response_access_token = SecretBox::new(Box::new(secret_response_access_token));

            println!("  {YELLOW}Encrypt data and save file{RESET}");
            encrypt_and_save_file(&private_key_path_struct, &encrypted_path_struct, secret_response_access_token)?;
            return Ok(secret_access_token);
        }
        println!("  {YELLOW}Decrypt the file with the private key.{RESET}");
        let secret_response_access_token = decrypt_text_with_metadata(encrypted_text_with_metadata)?;
        let secret_access_token = SecretString::from(secret_response_access_token.expose_secret().access_token.clone());
        Ok(secret_access_token)
    }
}

fn authenticate_with_browser_and_save_file(
    client_id: &str,
    private_key_path_struct: &ende::PathStructInSshFolder,
    encrypted_path_struct: &ende::PathStructInSshFolder,
) -> Result<SecretString> {
    let secret_response_access_token: SecretBox<SecretResponseAccessToken> = authentication_with_browser(client_id)?;
    let secret_access_token = SecretString::from(secret_response_access_token.expose_secret().access_token.clone());
    println!("  {YELLOW}Encrypt data and save file{RESET}");

    encrypt_and_save_file(private_key_path_struct, encrypted_path_struct, secret_response_access_token)?;
    Ok(secret_access_token)
}

/// Oauth2 PKCE code flow needs to be authenticated with a browser
/// <https://dropbox.tech/developers/pkce--what-and-why->
fn authentication_with_browser(client_id: &str) -> Result<SecretBox<SecretResponseAccessToken>> {
    // Step 1: Client creates code_verifier and subsequent code_challenge
    // generate a random 32 bytes, then encode to base64

    let mut random_bytes = [0_u8; 32];
    use aes_gcm::aead::rand_core::RngCore;
    aes_gcm::aead::OsRng.fill_bytes(&mut random_bytes);
    let code_verifier = <base64ct::Base64UrlUnpadded as base64ct::Encoding>::encode_string(&random_bytes);

    let code_challenge_bytes = rsa::sha2::Sha256::digest(code_verifier.as_bytes());
    let code_challenge = <base64ct::Base64UrlUnpadded as base64ct::Encoding>::encode_string(&code_challenge_bytes);

    // Step 2: Client sends code_challenge and code_challenge_method to /oauth2/authorize
    let url = format!("https://www.dropbox.com/oauth2/authorize?client_id={client_id}&response_type=code&token_access_type=offline&code_challenge={code_challenge}&code_challenge_method=S256");
    println!("  {YELLOW}Open authorization web page in browser and allow access: {RESET}");
    println!("{GREEN}{url}{RESET}");

    println!("  {YELLOW}Copy-paste the access_code:{RESET}");

    // the access code could be sent to to the redirect_uri. But I don't know if that makes it easier for me.
    // Instead the user can just copy paste from the browser. I think it needs to be done just once.
    let access_code = inquire::Password::new("").without_confirmation().prompt()?;

    // Step 4: Client sends authorization_code and code_verifier to /oauth2/token
    let params = [
        ("grant_type", "authorization_code"),
        ("code", &access_code),
        ("code_verifier", &code_verifier),
        ("client_id", client_id),
    ];

    let request = reqwest::blocking::Client::new()
        .post("https://api.dropbox.com/oauth2/token")
        .form(&params);

    let response = request.send()?;
    let response_str = response.text()?;

    /*
    response json
    {
    "access_token": "NW7lYmEWHgUAAAAAAAAAAbeutI8iL5CuBik9_CPD5r83XvcQPt-7O5diOdUUcsuX",
    "expires_in": 14399,
    "refresh_token": "xxxx",
    }
    */

    let secret_response_access_token: SecretResponseAccessToken = serde_json::from_str(&response_str)?;

    Ok(SecretBox::new(Box::new(secret_response_access_token)))
}

/// use refresh token to get new access_token and refresh_token
fn refresh_tokens(client_id: &str, refresh_token: String) -> Result<SecretBox<SecretResponseAccessToken>> {
    // https://developers.dropbox.com/oauth-guide#implementing-oauth

    #[derive(serde::Serialize)]
    struct RequestWithRefreshToken {
        client_id: String,
        grant_type: String,
        refresh_token: String,
    }
    // TODO: refresh for dropbox
    println!("  {YELLOW}Send request with refresh_token and retrieve access tokens{RESET}");
    println!("  {YELLOW}wait...{RESET}");

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", client_id),
    ];

    let request = reqwest::blocking::Client::new()
        .post("https://api.dropbox.com/oauth2/token")
        .form(&params);

    let response = request.send()?;
    let response_str = response.text()?;
    let secret_response_refresh_token: SecretResponseAccessToken = serde_json::from_str(&response_str)?;

    Ok(SecretBox::new(Box::new(secret_response_refresh_token)))
}

/// encrypt and save file
///
/// The "seed" are just some random 32 bytes.
/// The "seed" will be "signed" with the private key.
/// Only the "owner" can unlock the private key and sign correctly.
/// This signature will be used as the true passcode for symmetrical encryption.
/// The "seed" and the private key path will be stored in plain text in the file
/// together with the encrypted data in json format.
/// To avoid plain text in the end encode in base64 just for obfuscate a little bit.
fn encrypt_and_save_file(
    private_key_path_struct: &ende::PathStructInSshFolder,
    encrypted_path_struct: &ende::PathStructInSshFolder,
    secret_response_access_token: SecretBox<SecretResponseAccessToken>,
) -> Result<()> {
    let secret_string = SecretString::from(serde_json::to_string(&secret_response_access_token.expose_secret())?);

    let (plain_seed_bytes_32bytes, plain_seed_string) = crate::encrypt_decrypt_mod::random_seed_32bytes_and_string()?;

    println!("  {YELLOW}Unlock private key to encrypt the secret symmetrically{RESET}");
    let secret_passcode_32bytes: SecretBox<[u8; 32]> =
        ende::sign_seed_with_ssh_agent_or_private_key_file(private_key_path_struct, plain_seed_bytes_32bytes)?;

    println!("  {YELLOW}Encrypt the secret symmetrically {RESET}");
    let encrypted_string = ende::encrypt_symmetric(secret_passcode_32bytes, secret_string)?;

    // the file will contain json with 3 plain text fields: fingerprint, seed, encrypted, expiration

    // calculate expiration minus 10 minutes or 600 seconds
    let utc_now = chrono::Utc::now();
    let access_token_expiration = utc_now
        .checked_add_signed(chrono::Duration::seconds(
            secret_response_access_token.expose_secret().expires_in - 600,
        ))
        .ok_or_else(|| Error::ErrorFromStr("checked_add_signed"))?
        .to_rfc3339();

    let encrypted_text_with_metadata = ende::EncryptedTextWithMetadata {
        private_key_file_name: private_key_path_struct.get_file_name().to_string(),
        plain_seed_string,
        plain_encrypted_text: encrypted_string,
        access_token_expiration: Some(access_token_expiration),
        token_name: None,
    };
    let plain_file_text = serde_json::to_string_pretty(&encrypted_text_with_metadata)?;
    // encode it just to obscure it a little bit
    let file_text = ende::encode64_from_string_to_string(&plain_file_text);

    let mut file = std::fs::File::create(encrypted_path_struct.get_full_file_path())?;
    #[cfg(target_family = "unix")]
    {
        let metadata = file.metadata()?;
        let mut permissions = metadata.permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o600);
    }
    std::io::Write::write_all(&mut file, file_text.as_bytes())?;

    println!("  {YELLOW}Encrypted text saved to file.{RESET}");

    Ok(())
}

/// decrypt text with metadata
///
/// The encrypted file is encoded in base64 just to obfuscate it a little bit.  
/// In json format in plain text there is the "seed", the private key path and the encrypted secret.  
/// The "seed" will be "signed" with the private key.  
/// Only the "owner" can unlock the private key and sign correctly.  
/// This signature will be used as the true passcode for symmetrical decryption.  
fn decrypt_text_with_metadata(
    encrypted_text_with_metadata: ende::EncryptedTextWithMetadata,
) -> Result<SecretBox<SecretResponseAccessToken>> {
    // the private key file is written inside the file
    let private_key_path_struct = ende::PathStructInSshFolder::new(encrypted_text_with_metadata.private_key_file_name.clone())?;
    if !private_key_path_struct.exists() {
        return Err(Error::ErrorFromStr(
            "{RED}Error: File {private_key_path_struct} does not exist! {RESET}",
        ));
    }

    let plain_seed_bytes_32bytes = ende::decode64_from_string_to_32bytes(&encrypted_text_with_metadata.plain_seed_string)?;
    // first try to use the private key from ssh-agent, else use the private file with user interaction
    let secret_passcode_32bytes: SecretBox<[u8; 32]> =
        ende::sign_seed_with_ssh_agent_or_private_key_file(&private_key_path_struct, plain_seed_bytes_32bytes)?;
    // decrypt the data
    let decrypted_string = ende::decrypt_symmetric(secret_passcode_32bytes, encrypted_text_with_metadata.plain_encrypted_text)?;
    // parse json to struct
    let secret_response_access_token: SecretBox<SecretResponseAccessToken> =
        SecretBox::new(Box::new(serde_json::from_str(decrypted_string.expose_secret())?));
    Ok(secret_response_access_token)
}
