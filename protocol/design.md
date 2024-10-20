# Finder (0xA9F4)

Finder is a protocol for asynchronous messaging in fully decentralized contexts. It is intended to be used for messaging and message board applications. In addition protocol can scale down to run on embedded devices powered by microcontrollers allowing for deployment one cheep devices that are accessible and hackable.

 Although the core design is not tied to any particular networking technology the initial implementation focuses on ESPNow which is a preparatory IoT wireless protocol on the ESP32 family of microcontrollers. ESPNow was chosen based on it's range, ~500m line of site, and megabit speeds. Where choosing a priority protocol is not ideal it is outwaited by the ubiquity of low cost dev boards that can be acquired for between $5-$10 USD (as of 2024).

> ## Why Async?
>
> There have been many attempts in the past to build community wireless networks but none have seemed to gain traction. This is likely due to the fact that adhoc networks tend to have intermittent links and network splits. This will be true of any approach that doesn't invest large amounts of money and time in to infrastructure.
>
>Modern internet protocols such as TCP and HTTP do not work well (or at all) in environments lacking high availability networking, leading to frustrated users and lack of adoption. 
>
>If instead network protocols use an async modal applications can work reliably even when network access is only periodic.
>
>We believe the ability to operate opportunistically will solve the user experience issues that plage past attempts at community networks.

The protocol is composed of five main components:

- Messaging data modal
- User onboarding
- User management
- Data Syncing
- Data Retention

## Messaging modal

Messaging in Finder happens in channels. A channel is a async, multi reader, multi writer, message buss much like [AMQP](https://en.wikipedia.org/wiki/Advanced_Message_Queuing_Protocol).

 Finder differs from other message buss protocols in that it choses to optimizes availability and partition tolerance over strong global consistency. (See [CAP](https://en.wikipedia.org/wiki/CAP_theorem))

```mermaid
---
title: Pick Two
---
flowchart LR
    c((Consistency))
    a((Availability))
    p((Partition
       Tolerance))
    c <--> a
    a <--> p
    p <--> c
```

### Consistency

Finder uses a relaxed ordering model in which not all devices will order all message consistently. Instead Finder offers the weaker granites:

- All messages from a single sender will be consistently ordered at every device which receives them.

- Given a message `M_n` sent from a device `D1`. Any device `D2` that receives `M_n` can trivially detect if any prior message was not received.

- If a message `M2` is in response to some other `M1`, then a well behaved device `D1` will send `M1` before `M2` to other devices. (Assuming that both `M1` was retained by `D1`.)

These three properties result it partial orders such as the fallowing:

```mermaid
flowchart BT
    subgraph Device 1
        M1[[Message 1]]
        M4[[Message 3b]]
        M7[[Message 6]]
    end
    subgraph Device 2
        M2[[Message 2]]
        M6[[Message 5]]
    end
    subgraph Device 3
        M3[[Message 3a]]
        M5[[Message 4]]
    end

    M2 --> M1
    M3 --> M2

    M4 --> M1
    M4 --> M2

    M5 --> M3
    M5 --> M4
    
    M6 --> M2
    M6 --> M5

    M7 --> M4
    M7 --> M6

```

In this example each message points to the the previous message from the same device as well as  one it was in reply to. 

There are a number of orderings given this graph:

| Device   | Consistent | Inconsistent | Remainder |
| :---     | :---:      | :---:        | :---:     |
| Device 1 | 1, 2       | 3b, 3a       | 4, 5, 6   |
| Device 2a| 1, 2       | 3a, 3b       | 4, 5, (6) |
| Device 2b| 1, 2       | 3b, 3a       | 4, 5, (6) |
| Device 3 | 1, 2       | 3a, 3b       | 4, (5, 6) |

(*Messages in `()` are ones which might or might not have been see by a device*)

Ignoring messages which might not hav been received we see two distinct orderings. Device 1 and 3 will see `3a` and `3b` in different orderings. Device 2 might see either ordering depending on which of the other devices it communicates with first.

For all devices it is possible to detect when inconsistent orderings exist and adjust the display of messages in a manner that minimizes confusion to users.

### Availability

Data is **stored locally** on devices. This provides 100% availability for writes, as well as for reads of any past messages that have been retained.

New messages from other devices will be received as soon as a device is able to communicate with **any other** device which has retained them.

 There is no requirement in the protocol for coordinating nodes or centralized infrastructure.

### Partition Tolerance

Unlike most messages busses there is no central broker to provide ordering or store and forward messaging. Instead all devices participating in a channel retain any messages which have seen as long storage limits allow.

When two devices have an opportunity to communicate they exchange messages filling in any gaps in history they may have.
