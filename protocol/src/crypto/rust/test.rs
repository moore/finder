use super::*;
const SIG_SIZE: usize = 256;

use rsa::pkcs1::{DecodeRsaPrivateKey};

const PRIVATE_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAt+15Q+QlwFThI33dHA4qCFSmX35CsJBOMKAAH8TzhoTl5TL+
sv9861tvxlMgY181VDyvZWcUYHIqToFZKEEeVox4t3VtrTciJlCpfWGjDXsWvLGo
V4ExSkTXBF1P4oe+JRc5dz3T7Wviwa7QN+Mt9IGsaL9Qtq4XpQY03UoKLIbgnxjW
r0kkWrRoF5vDDaxBC6UqkONAE6z+JbhBF1e9VFd/+1NWzj3Go8xFTVcvfykWBy7l
djqSdJmMK3WV7R7gikYtdOMRug0Bt7UvFM5JMpRtf7FSEG7khalyppqtBiSW3zzu
lo+Hulki1b8jr10W6KUS2rzKc+A91yqy6lEeNwIDAQABAoIBADptiu9BQ6jUjeyr
aBkoervIwE1nm6HhRaV2vnNZKo9aGnnz+Cs+tB1EH77d21UWAqfu2z0YQMXenofv
2TXLcerGlvaYrC2xbPzE9QKqiJSYvIFW4oZhuRnBwphVWDI7MvEvbobtsiwi8Jbc
hLKsTYX1x6JC3E4cAdDfpt2BTrgT2s/eOfTHhMVpErGAg/0Qljy/Vg7hUiIgyTED
/Y5mfe/RJHXsz4ekkg5EtdwHkVr35zfe3O9wWf2HAxGVWFrAX+CKD62CK/tDrawu
g/sSPi5wSTqUFtcNYZCeQUwkz4sWS4jrwkl2nKQ4G+lrfRPXSZ7tDBGJXtR2KZf0
WY00WYkCgYEA02T0wCnGmlFC5Jl+AisxHWVaqoUD+hVKPnmjpLlua+8Xx5Tz7SEi
KDyJy9O2Vz7366VyIW+4c+jpltoPqtHnEjIava1nqN8GtTyrFl5gQKuSOSBsnBnL
63rY4I5NvdDiwqDo9tHUDoYDmeNkSpaj/1i84EmxtbsPPYQib6Ghnk0CgYEA3rzT
yC5CvQf7Z6V5gOx1au2ULtaAOLuLDXZIAMM7z42qmQS8SEkmEw/Vr6gRGClBh6QT
ZYxKTOegwKUYVBeheV1Y5Nvu1Jd/IwfuEibBVrZR1AfyhB/HIQ4QLmo33LP/rcPj
dwi7pPHsi9FMDJ1cRc52lAu1FljeAp9R+VBcGJMCgYB/JUe4lOfpRVsQl+mccFIY
Ni/0RBECR+/h59OvbgCmVqZc2pBkXftnbBINUIdprmv7hgVBayrsPHjSzNGDksCC
xzQiRbwFbC9irtzQlW8bNpa6WXA566IlPjxXw/+qXYsmORYl7kq3eY+M7aIS4sw8
9yiTVn/WqG4gN+tmbTcCOQKBgQCKTExjGvYtUOt0q3YJ6sftIJ7FhkIO98ObFDoY
3yAf+yJV6G7PozuU0lwnuP8ENXmOsv2oK7dmkNtrQhcc/58vMBql3zknnvk90wqr
Eo0xPfsI3/Zguyp1B7pcV29gBhNW3S47Fp0MCXqKReYmXv6QCWXu/mXt/je7ARlw
58iHKQKBgF5Gbm9Vm9VslX1ip9Et6Sev6u56bopYqLFKhy4mVjgm7wMRlv5k+oTK
v7I4OZ5SdijZfALO8oaYd9gjSFhxUq9bA2YXxzl06JfLPNTG93QORySJKNnEQ91V
BHm4I4zAJFmYCL/mBGIjhDI5q7YM7aHpQsDIIrx84vFbJqJfrJem
-----END RSA PRIVATE KEY-----
";

pub fn get_test_keys() -> KeyPair<RsaPrivateKey, RsaPublicKey> {
    let private = RsaPrivateKey::from_pkcs1_pem(PRIVATE_KEY).expect("error reading key");
    let public = private.to_public_key();
    KeyPair { private, public }
}

//#[test]
fn test_make_keys() -> Result<(), ClientError> {
    // This is slooooooooooo uncomment to test
    let seed = [0; 128];
    let mut crypto = RustCrypto::new(&seed)?;
    let _key_pair = crypto.make_signing_keys()?;

    // Used to dump key
    //let encoded =  key_pair.private.to_pkcs1_pem(LineEnding::default()).unwrap();
    //dbg!(encoded);
    //assert!(false);
    Ok(())
}

#[test]
fn test_sign_verify() -> Result<(), ClientError> {
    let seed = [0; 128];
    let crypto = RustCrypto::new(&seed)?;
    let key_pair = get_test_keys();

    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    let to = Recipient::Node(node2);

    let mut state: ChannelState<3, RsaPublicKey> =
        ChannelState::new(node1, key_pair.public.clone())?;

    let envelope = state.address(node1, 0)?;

    let mut target = [0u8; 4000];
    let sealed_envelope: SealedEnvelope<i32, 1025, SIG_SIZE> =
        crypto.seal(node1, to, &key_pair, &envelope, &mut target)?;

    let opened = crypto.open(&key_pair.public, &sealed_envelope)?;

    assert_eq!(envelope, opened);
    Ok(())
}

#[test]
fn test_envlope_id() -> Result<(), ClientError> {
    let seed = [0; 128];
    let crypto = RustCrypto::new(&seed)?;
    let key_pair = get_test_keys();

    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    let to = Recipient::Node(node2);

    let mut state: ChannelState<3, RsaPublicKey> =
        ChannelState::new(node1, key_pair.public.clone())?;

    let envelope = state.address(node1, 0)?;

    let mut target = [0u8; 4000];
    let sealed_envelope: SealedEnvelope<i32, 1025, SIG_SIZE> =
        crypto.seal(node1, to, &key_pair, &envelope, &mut target)?;

    let envlope_id1 = crypto.envelope_id(&sealed_envelope);
    let envlope_id2 = crypto.envelope_id(&sealed_envelope);

    assert_eq!(envlope_id1, envlope_id2);

    state.add_node(node2, key_pair.public.clone());

    let envelope2 = state.address(node2, 0)?;
    let sealed_envelope2: SealedEnvelope<i32, 1025, SIG_SIZE> =
        crypto.seal(node2, to, &key_pair, &envelope2, &mut target)?;

    let envlope_id3 = crypto.envelope_id(&sealed_envelope2);

    assert_ne!(envlope_id1, envlope_id3);

    Ok(())
}

#[test]
fn test_compute_id() -> Result<(), ClientError> {
    let key_pair = get_test_keys();

    let _device_id = RustCrypto::compute_id(&key_pair.public);

    Ok(())
}

#[test]
fn test_nonce() -> Result<(), CryptoError> {
    let seed = [0; 128];
    let mut crypto = RustCrypto::new(&seed)?;
    let nonce: u128 = crypto.nonce();
    let nonce2: u128 = crypto.nonce();

    assert_ne!(nonce, nonce2);

    Ok(())
}
