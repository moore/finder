#![no_std]

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(u128);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvelopeId(u128);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelId(u128);

#[derive(Copy, Clone)]
enum Recipient {
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
/// `sequence` be exactly one larger then the sequence
/// of the envelope specified by the `cause` field and
/// must also be strictly greater than the `last_sender`
/// field. The `last_sender` field must contain the `sequence`
/// value of the last `Envelope` produced by the sending node.
///
/// In the case that the sender knows of no existing messages sent
/// to a recipient the cause field should `EnvelopeId(0)` which is
/// virtual and has a implicit `sequence of 0;
///
/// When selecting a `cause` the sending node should choose the `Envelope`
/// with the largest `sequence` value or the envelope in the case that there
/// is a tie between two or more `Envelope`s for largest `sequence`.
pub struct Envelope<T: Sized> {
    from: NodeId,
    to: Recipient,
    cause: EnvelopeId,
    sender_last: u64,
    sequence: u64,
    data: T,
    signature: [u8; 128], // Should be what ever the ESP32 RSA HW implements.
}

impl<T: Sized> Envelope<T> {
    fn id(&self) -> EnvelopeId {
        EnvelopeId(0) //BOOG
    }
}

#[derive(Copy, Clone)]
struct NodeSequence {
    node: NodeId,
    id: EnvelopeId,
    sequence: u64,
}
pub struct ChannelState<const MAX: usize> {
    node_count: usize,
    nodes: [NodeSequence; MAX],
    newest: usize,
}

pub enum ChannelError {
    ClientMax(usize),
    MissingFromSender { have: u64, missing: u64 },
}

impl<const MAX: usize> ChannelState<MAX> {
    pub fn new() -> Self {
        let zero = NodeSequence {
            node: NodeId(0),
            id: EnvelopeId(0),
            sequence: 0,
        };
        Self {
             node_count: 0,
            nodes: [zero; MAX],
            newest: 0,
        }
    }

    pub fn receive<T>(&mut self, envelope: &Envelope<T>) -> Result<(), ChannelError> {
        let from = envelope.from;
        let in_use = &self.nodes[0..self.node_count];
        let pos = in_use.binary_search_by_key(&from, |&ns| ns.node);

        let index = match pos {
            Ok(index) => {
                let record = &mut self.nodes[index];

                if record.sequence != envelope.sender_last {
                    return Err(ChannelError::MissingFromSender {
                        have: record.sequence,
                        missing: envelope.sender_last,
                    });
                }

                record.sequence = envelope.sequence;
                index
            }

            Err(index) => {
                if self.node_count >= MAX {
                    return Err(ChannelError::ClientMax(MAX));
                }

                if envelope.sender_last != 0 {
                    return Err(ChannelError::MissingFromSender {
                        have: 0,
                        missing: envelope.sender_last,
                    });
                }

                self.nodes[index..].rotate_right(1);
                self.nodes[index] = NodeSequence {
                    node: from,
                    id: envelope.id(),
                    sequence: envelope.sequence,
                };
                index
            }
        };

        let current = &self.nodes[self.newest];

        // Updated newest if needed.
        if current.sequence < envelope.sequence {
            self.newest = index;
        } else if current.sequence == envelope.sequence && current.id < envelope.id() {
            self.newest = index;
        }

        Ok(())
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_channel_state() {
        let state: ChannelState<3> = ChannelState::new();
    }
}
