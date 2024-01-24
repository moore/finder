use super::*;

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum Recipient {
    Node(NodeId),
    Channel(ChannelId),
}

impl Recipient {
    pub fn to_be_bytes(&self) -> [u8; SHA256_SIZE] {
        match self {
            Recipient::Node(node_id) => node_id.to_be_bytes(),
            Recipient::Channel(channel_id) => channel_id.to_be_bytes(),
        }
    }
}

/// Each new message records the sender, recipient
/// relative order.
///
/// The sender is specified in the `from` field and
/// verified by checking the signature.
///
/// The recipient is specified in the `to` field as
/// either a specific device (node), or a channel.
///
/// The order is defined over the `cause`, `sequence`,
/// and `sender_last` fields. It is required that the
/// `sequence` be no larger then one more then the
/// largest sequence of the last envelope received
/// from the sender in the `cause` field and
/// must also be strictly greater than the `last_sender`
/// field. The `last_sender` field must contain the `sequence`
/// value of the last `Envelope` produced by the sending node.
///
/// The reason that we use the cause sequence and
/// associated constraints is to prevent a sender
/// setting a very large sequence and exhausting the sequence counter.
///
/// In the case that the sender knows of no existing messages sent
/// to a recipient the cause field should `EnvelopeId(0)` which is
/// virtual and has a implicit `sequence of 0;
///
/// When selecting a `cause` the sending node should choose the `Envelope`
/// with the largest `sequence` value or the envelope in the case that there
/// is a tie between two or more `Envelope`s for largest `sequence`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Message<T> {
    cause: NodeId,
    sender_last: u64,
    sequence: u64,
    pub(crate) data: T,
}

#[derive(Debug, Copy, Clone)]
pub struct NodeSequence<P> {
    pub public_key: P,
    pub node: NodeId,
    id: EnvelopeId,
    pub first_sequence: u64,
    pub sequence: u64,
}

#[derive(Debug)]
pub enum ChannelError {
    ClientMax(usize),
    MissingFromSender {
        node: NodeId,
        have: u64,
        missing: u64,
    },
    SequenceOverFlow,
    Unreachable,
    NodeExists,
    UnknownNode,
    AlreadyReceived,
}

#[derive(Debug)]
pub struct ChannelState<const MAX_NODES: usize, P> {
    nodes: Vec<NodeSequence<P>, { MAX_NODES }>,
    newest: NodeId,
}

impl<const MAX_NODES: usize, P: Clone> ChannelState<MAX_NODES, P> {
    pub fn new(initial: NodeId, node_key: P) -> Result<Self, ChannelError> {
        let mut nodes = Vec::new();

        let initial_record = NodeSequence {
            public_key: node_key,
            node: initial,
            id: EnvelopeId::new(0),
            sequence: 0,
            first_sequence: 0,
        };

        if let Err(_) = nodes.push(initial_record) {
            return Err(ChannelError::ClientMax(MAX_NODES));
        }

        Ok(Self {
            nodes,
            newest: initial,
        })
    }

    pub fn list_nodes(&self) -> &[NodeSequence<P>] {
        &self.nodes
    }

    pub fn add_node(&mut self, node: NodeId, node_key: P) -> Result<(), ChannelError> {
        let pos = self.nodes.binary_search_by_key(&node, |ns| ns.node);

        match pos {
            Ok(index) => return Err(ChannelError::NodeExists),

            Err(index) => {
                let record = NodeSequence {
                    public_key: node_key,
                    node: node,
                    id: EnvelopeId::new(0),
                    sequence: 0,
                    first_sequence: 0,
                };

                if let Err(_) = self.nodes.insert(index, record) {
                    return Err(ChannelError::ClientMax(MAX_NODES));
                };
            }
        };

        Ok(())
    }

    pub fn get_node_key(&self, node: NodeId) -> Result<P, ChannelError> {
        let pos = self.nodes.binary_search_by_key(&node, |ns| ns.node);

        match pos {
            Ok(index) => {
                let record = self.nodes.get(index).ok_or(ChannelError::Unreachable)?;
                Ok(record.public_key.clone())
            }

            Err(index) => Err(ChannelError::UnknownNode),
        }
    }

    pub fn receive<T: Serialize>(
        &mut self,
        from: NodeId,
        message: &Message<T>,
        id: &EnvelopeId,
    ) -> Result<u64, ChannelError> {
        let index = self.check_receive_worker(from, message, id)?;

        let current = self.get_current()?;

        let max_sequence: u64;
        // Updated newest if needed.
        if current.sequence < message.sequence
            || (current.sequence == message.sequence && current.id < *id)
        {
            max_sequence = message.sequence;
            self.newest = from;
        } else {
            max_sequence = current.sequence;
        }

        let record_mut = self.nodes.get_mut(index).ok_or(ChannelError::Unreachable)?;

        record_mut.sequence = message.sequence;
        record_mut.id = *id;

        if record_mut.first_sequence == 0 {
            record_mut.first_sequence = message.sequence;
        }

        Ok(max_sequence)
    }

    pub fn check_receive<T: Serialize>(
        &mut self,
        from: NodeId,
        envelope: &Message<T>,
        id: &EnvelopeId,
    ) -> Result<(), ChannelError> {
        self.check_receive_worker(from, envelope, id)?;
        Ok(())
    }

    fn check_receive_worker<T: Serialize>(
        &mut self,
        from: NodeId,
        message: &Message<T>,
        id: &EnvelopeId,
    ) -> Result<usize, ChannelError> {
        let pos = self.nodes.binary_search_by_key(&from, |ns| ns.node);

        let index = match pos {
            Ok(index) => index,
            Err(index) => {
                return Err(ChannelError::UnknownNode);
            }
        };

        let record = self.nodes.get(index).ok_or(ChannelError::Unreachable)?;

        // check that the sequence last matches
        if record.sequence < message.sender_last {
            return Err(ChannelError::AlreadyReceived);
        } else if record.sequence != message.sender_last {
            return Err(ChannelError::MissingFromSender {
                node: from,
                have: record.sequence,
                missing: message.sender_last,
            });
        }

        let cause_pos = self
            .nodes
            .binary_search_by_key(&message.cause, |ns| ns.node);
        let cause_target = message.sequence.saturating_sub(1);

        match cause_pos {
            Ok(index) => {
                let cause = self.nodes.get(index).ok_or(ChannelError::Unreachable)?;
                if cause.sequence < cause_target {
                    return Err(ChannelError::MissingFromSender {
                        node: cause.node,
                        have: cause.sequence,
                        missing: cause_target,
                    });
                }
            }
            Err(_) => {
                if cause_target != 0 {
                    return Err(ChannelError::MissingFromSender {
                        node: message.cause,
                        have: 0,
                        missing: cause_target,
                    });
                }
            }
        }

        Ok(index)
    }

    pub fn address<T: Serialize>(
        &mut self,
        from: NodeId,
        data: T,
    ) -> Result<Message<T>, ChannelError> {
        let pos = self.nodes.binary_search_by_key(&from, |ns| ns.node);

        let record = match pos {
            Ok(index) => &self.nodes[index],

            Err(index) => return Err(ChannelError::UnknownNode),
        };

        let current = self.get_current()?;

        let last_sequence = match record.sequence > current.sequence {
            true => record.sequence,
            false => current.sequence,
        };

        let Some(sequence) = last_sequence.checked_add(1) else {
            return Err(ChannelError::SequenceOverFlow);
        };

        let result = Message {
            cause: current.node,
            sender_last: record.sequence,
            sequence,
            data,
        };

        Ok(result)
    }

    fn get_current<'a>(&'a self) -> Result<&'a NodeSequence<P>, ChannelError> {
        let id = &self.newest;
        let Ok(pos) = self.nodes.binary_search_by_key(id, |ns| ns.node) else {
            return Err(ChannelError::Unreachable);
        };

        let result = self.nodes.get(pos).ok_or(ChannelError::Unreachable)?;

        Ok(result)
    }
}

#[cfg(test)]
mod test;
