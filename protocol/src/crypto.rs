use super::*;

use core::fmt;

pub const SHA256_SIZE: usize = 32; //bytes
pub const RSA_KEY_SIZE: usize = 256; //bytes

pub mod rust;

#[derive(Hash, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id {
    pub data: [u8; SHA256_SIZE],
}

impl Id {
    fn to_be_bytes(&self) -> [u8; SHA256_SIZE] {
        self.data.clone()
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Id: ")?;
        for b in &self.data {
            write!(f, "{:x?}", *b)?;
        }
        Ok(())
    }    
}

impl From<u8> for Id {
    fn from(value: u8) -> Self {
        Self {
            data: [value; SHA256_SIZE],
        }
    }
}

impl From<[u8; SHA256_SIZE]> for Id {
    fn from(value: [u8; SHA256_SIZE]) -> Self {
        Self { data: value }
    }
}

#[derive(Debug, Hash, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(Id);

impl NodeId {
    pub fn new<T>(value: T) -> Self
    where
        T: Into<Id>,
    {
        Self(value.into())
    }

    pub fn to_be_bytes(&self) -> [u8; SHA256_SIZE] {
        self.0.to_be_bytes()
    }
}

#[derive(Debug, Hash, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvelopeId(Id);

impl EnvelopeId {
    pub fn new<T>(value: T) -> Self
    where
        T: Into<Id>,
    {
        Self(value.into())
    }

    pub fn to_be_bytes(&self) -> [u8; SHA256_SIZE] {
        self.0.to_be_bytes()
    }
}

#[derive(Hash, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelId(Id);

impl fmt::Debug for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChannelId: ")?;
        for b in &self.0.data {
            write!(f, "{:x?}", *b)?;
        }
        Ok(())
    }    
}

impl ChannelId {
    pub fn new<T>(value: T) -> Self
    where
        T: Into<Id>,
    {
        Self(value.into())
    }

    pub fn to_be_bytes(&self) -> [u8; SHA256_SIZE] {
        self.0.to_be_bytes()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct KeyPair<S, P> {
    pub public: P,
    pub private: S,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SealedEnvelope<T, const MAX_ENVELOPE: usize, const MAX_SIG: usize> {
    pub from: NodeId,
    pub to: Recipient,
    pub serialized: Vec<u8, MAX_ENVELOPE>,
    pub signature: Vec<u8, MAX_SIG>,
    _phantom: PhantomData<T>,
}

impl<T, const MAX_ENVELOPE: usize, const MAX_SIG: usize> SealedEnvelope<T, MAX_ENVELOPE, MAX_SIG> {
    pub fn new(
        from: NodeId,
        to: Recipient,
        serialized: &[u8],
        signature: &[u8],
    ) -> Result<Self, CryptoError> {
        let Ok(data_vec) = Vec::from_slice(serialized) else {
            return Err(CryptoError::MaxEnvelope);
        };
        let Ok(sig_vec) = Vec::from_slice(signature) else {
            return Err(CryptoError::MaxSig);
        };
        Ok(Self {
            from,
            to,
            serialized: data_vec,
            signature: sig_vec,
            _phantom: PhantomData::<T>,
        })
    }

    pub fn id(&self, crypto: &impl Crypto) -> EnvelopeId {
        crypto.envelope_id(self)
    }

    pub fn from(&self) -> NodeId {
        self.from
    }
}

#[derive(Debug)]
pub enum CryptoError {
    PostcardError(postcard::Error),
    Unreachable,
    InternalError,
    MaxEnvelope,
    MaxSig,
    VerifyError,
}

impl From<postcard::Error> for CryptoError {
    fn from(value: postcard::Error) -> Self {
        CryptoError::PostcardError(value)
    }
}

impl From<rsa::pkcs8::Error> for CryptoError {
    fn from(_value: rsa::pkcs8::Error) -> Self {
        CryptoError::InternalError
    }
}

pub trait Crypto {
    type PubSigningKey: Clone + Serialize + DeserializeOwned;
    type PrivateSigningKey: Clone + Serialize + DeserializeOwned;

    fn compute_id(key: &Self::PubSigningKey) -> NodeId;

    fn get_id<
        T: Serialize + for<'a> Deserialize<'a>,
        const MAX_ENVELOPE: usize,
        const MAX_SIG: usize,
    >(
        _sealed_envlope: &SealedEnvelope<T, MAX_ENVELOPE, MAX_SIG>,
    ) -> NodeId {
        unimplemented!()
    }

    fn envelope_id<T, const MAX_ENVELOPE: usize, const MAX_SIG: usize>(
        &self,
        sealed: &SealedEnvelope<T, MAX_ENVELOPE, MAX_SIG>,
    ) -> EnvelopeId;

    fn seal<
        T: Serialize + for<'a> Deserialize<'a>,
        const MAX_ENVELOPE: usize,
        const MAX_SIG: usize,
    >(
        &self,
        from: NodeId,
        to: Recipient,
        key_pair: &KeyPair<Self::PrivateSigningKey, Self::PubSigningKey>,
        envelope: &Message<T>,
        target: &mut [u8],
    ) -> Result<SealedEnvelope<T, MAX_ENVELOPE, MAX_SIG>, CryptoError>;

    fn open<T: Serialize + DeserializeOwned, const MAX_ENVELOPE: usize, const MAX_SIG: usize>(
        &self,
        key: &Self::PubSigningKey,
        sealed_envelope: &SealedEnvelope<T, MAX_ENVELOPE, MAX_SIG>,
    ) -> Result<Message<T>, CryptoError>;

    fn nonce(&mut self) -> u128;

    fn make_signing_keys(
        &mut self,
    ) -> Result<KeyPair<Self::PrivateSigningKey, Self::PubSigningKey>, CryptoError>;

    fn channel_id_from_bytes(&self, data: &[u8]) -> ChannelId;
}
