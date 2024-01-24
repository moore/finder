

use super::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct Clock {
    pub node: NodeId,
    pub sequence: u64,
}

impl Clock {
    pub fn new(node: NodeId, sequence: u64) -> Self {
        Self { node, sequence }
    }
}

#[derive(Debug)]
pub struct SyncResponderState<const MAX_NODES: usize> {
    pub session_id: u32,
    pub bytes_budget: u32,
    pub bytes_sent: u32,
    pub last_command_index: u64,
    pub vector_clock: Vec<Clock, MAX_NODES>,
}

impl<const MAX_NODES: usize> SyncResponderState<MAX_NODES> {
    pub fn get_min_sequence(&self) -> Option<u64> {
        let mut min = match self.vector_clock.get(0) {
            Some(clock) => clock.sequence,
            None => return None,
        };

        for clock in &self.vector_clock {
            if min > clock.sequence {
                min = clock.sequence;
            }
        }

        Some(min)
    }
}

#[derive(Debug)]
pub struct SyncRequesterState {
    pub session_id: u32,
    pub bytes_budget: u32,
    pub bytes_received: u32,
    pub last_received_timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncRequest<const MAX_NODES: usize> {
    pub session_id: u32,
    pub bytes_budget: u32,
    pub vector_clock: Vec<Clock, MAX_NODES>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResponse<const RESPONSE_MAX: usize> {
    pub session_id: u32,
    pub data: Vec<u8, RESPONSE_MAX>,
}
