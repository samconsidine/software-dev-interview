# Software Dev Interview

## Setup

This exercise uses the hosted interview router:

- Sender UDP target: `interview-router.adamohq.com:9001`
- Browser WebTransport endpoint: `https://interview-router.adamohq.com`
- Scenario API: `https://interview-router.adamohq.com/config`

Chrome needs a network path that allows QUIC/WebTransport over UDP `443`.
If the page shows `ERR_QUIC_PROTOCOL_ERROR` or `QUIC_NETWORK_IDLE_TIMEOUT`,
try a different network or disable a VPN/firewall that blocks UDP `443`.

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
