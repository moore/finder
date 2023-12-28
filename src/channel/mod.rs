
use super::*;

#[derive(Debug, Copy, Clone)]
struct NodeSequence {
    node: NodeId,
    id: EnvelopeId,
    sequence: u64,
}

#[derive(Debug)]
pub struct ChannelState<const MAX: usize> {
    nodes: Vec<NodeSequence, {MAX}>,
    newest: NodeId,
}
#[derive(Debug)]
pub enum ChannelError {
    ClientMax(usize),
    MissingFromSender { node: NodeId, have: u64, missing: u64 },
    SequenceOverFlow,
    Unreachable,
}

impl<const MAX: usize> ChannelState<{MAX}> {
    pub fn new(initial: NodeId) -> Result<Self, ChannelError> {
        let mut nodes = Vec::new();

        let initial_record = NodeSequence{
            node: initial,
            id: EnvelopeId(0), 
            sequence: 0,
        };

        if let Err(_) = nodes.push(initial_record) {
            return Err(ChannelError::ClientMax(MAX));
        }

        Ok(Self {
            nodes,
            newest: initial,
        })
    }

    pub fn receive<T>(&mut self, envelope: &Envelope<T>, id: &EnvelopeId) -> Result<(), ChannelError> {
        let from = envelope.from;
        let pos = self.nodes.binary_search_by_key(&from, |&ns| ns.node);

        let index = match pos {
            Ok(index) => {
                let record = &mut self.nodes.get_mut(index)
                    .ok_or(ChannelError::Unreachable)?;

                index
            }

            Err(index) => {
                let record = NodeSequence {
                    node: from,
                    id: id.clone(),
                    sequence: envelope.sequence,
                };

                if let Err(_) =self.nodes.insert(index, record) {
                    return Err(ChannelError::ClientMax(MAX))
                };

                index
            }
        };

        let record = self.nodes.get(index)
            .ok_or(ChannelError::Unreachable)?;

        // check that the sequence last matches
        if record.sequence != envelope.sender_last {
            return Err(ChannelError::MissingFromSender {
                node: envelope.from,
                have: record.sequence,
                missing: envelope.sender_last,
            });
        }

        let cause_pos = self.nodes.binary_search_by_key(&envelope.cause, |&ns| ns.node);
        let cause_target = envelope.sequence.saturating_sub(1);

        match cause_pos {
                Ok(index) => {
                    let cause = self.nodes.get(index)
                        .ok_or(ChannelError::Unreachable)?;
                    if cause.sequence < cause_target {

                        return Err(ChannelError::MissingFromSender {
                            node: cause.node,
                            have: cause.sequence,
                            missing: cause_target,
                        });
                    }

                },
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
    

        let current = self.get_current()?;

        // Updated newest if needed.
        if current.sequence < envelope.sequence 
        || (current.sequence == envelope.sequence && current.id < *id){
            self.newest = envelope.from;
        } 

        let record_mut = self.nodes.get_mut(index)
        .ok_or(ChannelError::Unreachable)?;


        record_mut.sequence = envelope.sequence;
        record_mut.id       = *id;

        Ok(())
    }

    

    pub fn address<T>(&mut self, from: NodeId, to: Recipient, data: T) -> Result<Envelope<T>, ChannelError> {

        let pos = self.nodes.binary_search_by_key(&from, |&ns| ns.node);

        let record = match pos {
            Ok(index) =>  &self.nodes[index],
            

            Err(index) => {
                let record = NodeSequence {
                    node: from,
                    id: EnvelopeId(0),
                    sequence: 0,
                };

                if let Err(_) = self.nodes.insert(index, record) {
                    return Err(ChannelError::ClientMax(MAX))
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
            return Err(ChannelError::SequenceOverFlow)
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
        let Ok(pos) =  self.nodes.binary_search_by_key(id, |&ns| ns.node) else {
            return Err(ChannelError::Unreachable)
        };

        let result = self.nodes.get(pos)
            .ok_or(ChannelError::Unreachable)?;

        Ok(result)
    }
}



#[cfg(test)]
mod test;