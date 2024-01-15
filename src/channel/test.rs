use super::*;

#[test]
fn new_channel_state() -> Result<(), ChannelError> {
    let _state: ChannelState<3> = ChannelState::new(NodeId::new(1))?;
    Ok(())
}

#[test]
fn address_envelope() -> Result<(), ChannelError> {
    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    let to = Recipient::Node(node2);

    let mut state: ChannelState<3> = ChannelState::new(node1)?;

    let envlope = state.address(node1, 0)?;

    assert_eq!(envlope.cause, node1);
    assert_eq!(envlope.sender_last, 0);
    assert_eq!(envlope.sequence, 1);

    Ok(())
}

#[test]
fn receive_envelope() -> Result<(), ChannelError> {
    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    let to = Recipient::Node(node2);

    let mut state: ChannelState<3> = ChannelState::new(node1)?;

    let envlope1 = state.address(node1, 0)?;
    let envlope1_id = EnvelopeId::new(1);

    state.receive::<i32>(node1, &envlope1, &envlope1_id)?;

    let record = state.get_current()?;

    assert_eq!(record.id, envlope1_id);

    Ok(())
}

#[test]
fn many_envelope() -> Result<(), ChannelError> {
    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    let channel = ChannelId::new(3);
    let to = Recipient::Channel(channel);

    let mut state: ChannelState<3> = ChannelState::new(node1)?;

    let envlope1 = state.address(node1, 0)?;
    let envlope1_id = EnvelopeId::new(1);

    state.receive::<i32>(node1, &envlope1, &envlope1_id)?;

    let record = state.get_current()?;

    assert_eq!(record.id, envlope1_id);

    let envlope2 = state.address(node2, 0)?;
    let envlope2_id = EnvelopeId::new(2);

    assert_eq!(envlope2.cause, node1);
    assert_eq!(envlope2.sender_last, 0);
    assert_eq!(envlope2.sequence, 2);

    state.receive::<i32>(node2, &envlope2, &envlope2_id)?;

    let record = state.get_current()?;

    assert_eq!(record.id, envlope2_id);

    Ok(())
}
