use super::*;

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum Recipient {
    Node(NodeId),
    Channel(ChannelId),
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
pub struct Envelope<T> {
    from: NodeId,
    to: Recipient,
    cause: NodeId,
    sender_last: u64,
    sequence: u64,
    pub(crate) data: T,
}

#[derive(Debug, Copy, Clone)]
struct NodeSequence {
    node: NodeId,
    id: EnvelopeId,
    sequence: u64,
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
}

#[derive(Debug)]
pub struct ChannelState<const MAX_NODES: usize> {
    nodes: Vec<NodeSequence, { MAX_NODES }>,
    newest: NodeId,
}

impl<const MAX_NODES: usize> ChannelState<MAX_NODES> {
    pub fn new(initial: NodeId) -> Result<Self, ChannelError> {
        let mut nodes = Vec::new();

        let initial_record = NodeSequence {
            node: initial,
            id: EnvelopeId::new(0),
            sequence: 0,
        };

        if let Err(_) = nodes.push(initial_record) {
            return Err(ChannelError::ClientMax(MAX_NODES));
        }

        Ok(Self {
            nodes,
            newest: initial,
        })
    }

    pub fn receive<T: Serialize>(
        &mut self,
        envelope: &Envelope<T>,
        id: &EnvelopeId,
    ) -> Result<u64, ChannelError> {
        let index = self.check_receive_worker(envelope, id)?;

        let current = self.get_current()?;

        // Updated newest if needed.
        if current.sequence < envelope.sequence
            || (current.sequence == envelope.sequence && current.id < *id)
        {
            self.newest = envelope.from;
        }

        let record_mut = self.nodes.get_mut(index).ok_or(ChannelError::Unreachable)?;

        record_mut.sequence = envelope.sequence;
        record_mut.id = *id;

        let min_record = self
            .nodes
            .iter()
            .min_by_key(|n| n.sequence)
            .ok_or(ChannelError::Unreachable)?;

        Ok(min_record.sequence)
    }

    pub fn check_receive<T: Serialize>(
        &mut self,
        envelope: &Envelope<T>,
        id: &EnvelopeId,
    ) -> Result<(), ChannelError> {
        self.check_receive_worker(envelope, id)?;
        Ok(())
    }

    fn check_receive_worker<T: Serialize>(
        &mut self,
        envelope: &Envelope<T>,
        id: &EnvelopeId,
    ) -> Result<usize, ChannelError> {
        let from = envelope.from;
        let pos = self.nodes.binary_search_by_key(&from, |&ns| ns.node);

        let index = match pos {
            Ok(index) => {
                self.nodes.get(index).ok_or(ChannelError::Unreachable)?;

                index
            }

            Err(index) => {
                let record = NodeSequence {
                    node: from,
                    id: id.clone(),
                    sequence: envelope.sequence,
                };

                if let Err(_) = self.nodes.insert(index, record) {
                    return Err(ChannelError::ClientMax(MAX_NODES));
                };

                index
            }
        };

        let record = self.nodes.get(index).ok_or(ChannelError::Unreachable)?;

        // check that the sequence last matches
        if record.sequence != envelope.sender_last {
            return Err(ChannelError::MissingFromSender {
                node: envelope.from,
                have: record.sequence,
                missing: envelope.sender_last,
            });
        }

        let cause_pos = self
            .nodes
            .binary_search_by_key(&envelope.cause, |&ns| ns.node);
        let cause_target = envelope.sequence.saturating_sub(1);

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
                        node: envelope.cause,
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
        to: Recipient,
        data: T,
    ) -> Result<Envelope<T>, ChannelError> {
        let pos = self.nodes.binary_search_by_key(&from, |&ns| ns.node);

        let record = match pos {
            Ok(index) => &self.nodes[index],

            Err(index) => {
                let record = NodeSequence {
                    node: from,
                    id: EnvelopeId::new(0),
                    sequence: 0,
                };

                if let Err(_) = self.nodes.insert(index, record) {
                    return Err(ChannelError::ClientMax(MAX_NODES));
                };

                &self.nodes.get(index).ok_or(ChannelError::Unreachable)?
            }
        };

        let current = self.get_current()?;

        let last_sequence = match record.sequence > current.sequence {
            true => record.sequence,
            false => current.sequence,
        };

        let Some(sequence) = last_sequence.checked_add(1) else {
            return Err(ChannelError::SequenceOverFlow);
        };

        let result = Envelope {
            from,
            to,
            cause: current.node,
            sender_last: record.sequence,
            sequence,
            data,
        };

        Ok(result)
    }

    fn get_current<'a>(&'a self) -> Result<&'a NodeSequence, ChannelError> {
        let id = &self.newest;
        let Ok(pos) = self.nodes.binary_search_by_key(id, |&ns| ns.node) else {
            return Err(ChannelError::Unreachable);
        };

        let result = self.nodes.get(pos).ok_or(ChannelError::Unreachable)?;

        Ok(result)
    }
}

#[cfg(test)]
mod test;
