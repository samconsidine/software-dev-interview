# Software Dev Interview

## Setup

This exercise uses the hosted interview router:

- Sender UDP target: `interview-router.adamohq.com:9001`
- Browser WebTransport endpoint: `https://interview-router.adamohq.com`
- Scenario API: `https://interview-router.adamohq.com/config`

Chrome needs a network path that allows QUIC/WebTransport over UDP `443`.
If the page shows `ERR_QUIC_PROTOCOL_ERROR` or `QUIC_NETWORK_IDLE_TIMEOUT`,
try a different network or disable a VPN/firewall that blocks UDP `443`.

## Framing contract

The router treats each video message as:

```text
[4-byte big-endian payload length][payload bytes]
```

That length prefix is transport framing, not an interpretation of the payload.
It gives the router and browser message boundaries after bytes pass through
stream-oriented layers. The sender and browser may change the contents of
`payload bytes`, but the hosted relay depends on the outer length prefix.
Interviewees do not have access to the relay code, so treat the length prefix
as fixed.

### 1. Start the sender

```bash
cargo run
```

### 2. Start the frontend

```bash
cd web
npm install
npm run dev
```

Open http://localhost:5173 in Chrome. If Chrome has an old copy of the page,
hard-refresh so it reconnects to the current WebTransport endpoint.
