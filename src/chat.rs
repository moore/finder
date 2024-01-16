use super::*;

use heapless::FnvIndexMap;
use heapless::String;

const NAME_MAX: usize = 128;
const CHAT_MAX: usize = 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct NewChannel<P> {
    pub nonce: u128,
    pub name: String<NAME_MAX>,
    pub owner: P,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AddUser<P> {
    pub name: String<NAME_MAX>,
    pub key: P,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub text: String<CHAT_MAX>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Protocol<P> {
    AddUser(AddUser<P>),
    NewChannel(NewChannel<P>),
    ChatMessage(ChatMessage),
}

#[derive(Debug)]
pub enum ChatError {
    UnexpectedId,
    MaxUsersExceeded,
    Uninitlized,
    Unauthorized,
    Unreachable,
}

pub struct Chat<const MAX_USERS: usize, C: Crypto> {
    id: ChannelId,
    owner_id: Option<NodeId>,
    users: FnvIndexMap<NodeId, C::PubSigningKey, MAX_USERS>,
    message_count: u64,
    _phantom: PhantomData<C>,
}

pub enum AcceptResult<C: Crypto> {
    AddUser(C::PubSigningKey),
    NewMessage(u64),
    None,
}

impl<'a, const MAX_USERS: usize, C: Crypto> Chat<MAX_USERS, C> {
    pub fn new(id: ChannelId) -> Self {
        Self {
            id,
            owner_id: None,
            users: FnvIndexMap::new(),
            message_count: 0,
            _phantom: PhantomData::<C>,
        }
    }

    pub fn message_count(&self) -> u64 {
        self.message_count
    }

    pub fn accept_message(
        &mut self,
        id: ChannelId,
        author: NodeId,
        message: &Protocol<C::PubSigningKey>,
    ) -> Result<AcceptResult<C>, ChatError> {
        match message {
            Protocol::NewChannel(new_channel) => {
                if id != self.id {
                    return Err(ChatError::UnexpectedId);
                }

                let key = &new_channel.owner;
                // Do failable operation first.
                let owner_id = self.add_user(key)?;
                self.owner_id = Some(owner_id);

                Ok(AcceptResult::None)
            }
            Protocol::AddUser(add_user) => {
                let Some(owner_id) = &self.owner_id else {
                    return Err(ChatError::Uninitlized);
                };

                if author != *owner_id {
                    return Err(ChatError::Unauthorized);
                }

                self.add_user(&add_user.key)?;
                Ok(AcceptResult::AddUser(add_user.key.clone()))
            }
            Protocol::ChatMessage(chat_message) => {
                if !self.users.contains_key(&author) {
                    return Err(ChatError::Unauthorized);
                }

                self.message_count = self.message_count.checked_add(1)
                    .ok_or(ChatError::Unreachable)?;

                Ok(AcceptResult::NewMessage(self.message_count))
            }
        }
    }

    fn add_user<'b>(&mut self, key: &C::PubSigningKey) -> Result<NodeId, ChatError> {
        let id = C::compute_id(key);

        match self.users.insert(id.clone(), key.clone()) {
            Err(_) => Err(ChatError::MaxUsersExceeded),
            Ok(_) => Ok(id),
        }
    }
}
