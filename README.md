# soldr

A webhook delivery network

## Basic Example

Start the provided origin server: `cargo run --example origin`

In a separate terminal window, start soldr: `RUST_LOG=soldr=trace cargo run`

In a separate terminal window, run the following curl requests:

Map an origin:

```
curl -vvv -H "Content-Type: application/json" localhost:3443/origins \
-d '{ "domain": "example.wh.soldr.dev", "origin_uri": "http://localhost:8080" }'
```

Send a request that will succeed:

```
curl -vvv -H "Host: example.wh.soldr.dev" localhost:3000/ -d ''
```

Send a request that will failure:

```
curl -vvv -H "Host: example.wh.soldr.dev" localhost:3000/failure -d ''
```

This request will be retried repeatedly by the retry queue.


In the terminal window running soldr, you will see a lot of tracing output that demonstrates how soldr is running.

## Architecture

The goal is to have a single process that people can easily get running.

### Ingest

The ingest task uses the domain name to figure out the webhook service. Example: acme-orders.wh.soldr.dev

- The `wh` subdomain is what the ingest service listens on.
- The sub-subdomain contains tenant specific information. The `acme-orders` part is the webhook service identifier.
   - Users can include random characters, `acme-orders-239dsx` to prevent people from easily guessing valid domains.

The ingest task is designed to be very simple: it immediately saves the request to the queue. Currently, the queue is a SQLite table. If that fails, it will log the request to stdout. The intention is that we _never_ drop a request. If the database is down, we can parse the logs to restore missing requests.

Immediately saving the request means we may save requests that are not for any specific service. We will delete these later. In the future, we may decide to keep a cached list of valid subdomains.

### Processor

A separate task will read from the queue and attempt to send a request that has not yet been sent. Each _attempt_ to send a request will record the response body and the time the request was made.

If a request is successfully sent, then we will mark that request as complete and no longer try and process it.

If a request fails, we will mark the next time the request should be tried. We will implement exponential backoff. Each service will have a max number of attempts. Once those attempts are exhausted, the processor will send an alert. We mark the state as alerted so we can later send a resolved email.

If a request does not relate to any active account, mark it as completed.

Key points:

- we must take great care to avoid sending the same request multiple times (unless we are trying to re-send a failed request)

### Purge

Requests and attempts older than 30 days will be removed from the database. A task will remove these records in such a way that does not cause long-lived locks on the database. We may decide to add support for 90 days (or even longer) history later.

### Management API

There was some cases where manual intervention must take place in order to resolve a problem with webhook delivery.

We will expose an API that allows for a human to manually intervene. This allows us to present a UI to the user and they can re-send a request immediately.

The API will listen on a separate, non-standard port. For example, we may default that port to 8443. This makes it more difficult to accientally expose the management API to the internet. We will also enforce authentication (authn) using a shared secret.

### Management UI

We will create a simple management UI to allow two primary tasks:

- editing a webhook request
- re-sending the webhook request immediately

The UI can be run as part of the same process or run in a separate process. When running as a separate process, the UI will have to be configured with the URL of the Management API and the shared secret.

User authn/authz is out of scope for the Management API. We may add oauth support in the future.

### Logical Schema

- webhooks - a mapping of a subdomain to a webhook destination
- requests - each webhook request
- attempts - an attempt to send a request

## Future Considerations

### Metrics

We want to keep metrics around for a long time.

Key metrics:

- total number of requests
- total number of failed requests
- total number of requests that have been re-tried

We might also consider sending out open telemetry logs so the health of soldr can be monitored and users can alert if there are any reported errors. The errors may be with soldr itself or with the number of requests that were never successfully sent.

### Safe Restarts

A naive restart runs the risk that we lose a request. We can use systemd to buffer those requests during a restart.

## Appendix

### Webhook Failure Modes

- network failure - This can happen whether we are sending the request over the internet or using a VPN tunnel. The origin service tries to send a request to the destination and the data never reached the detstination service. The cause of the network failure may be related to the origin, the destination or even a third party. For example: requests are normally sent over the internet which involves transiting through Comcast's network. If Comcast has an issue, the requests will be dropped until network operators re-route traffic to another provider, such as Level3.
- unhealthy destination - This is the most common failure mode. This issue occurs when a request does not respond with a 2xx HTTP response code.
- invalid request - This occurs when the request does not conform to the destination's specification. The issue may be due to incorrect or mossing data from the HTTP body or the HTTP headers. For example: the request may be missing a `Content-Type: application/json` header or is sending a boolean as a string in the body `{ "active": "true" }` vs `{ "active": true }`. This error may not always be the fault of the origin service. The destination service may be updated in such a way that that past requests were successful, but new requests were not. For example: the destination service did not require a `Content-Type` header but suddenly does. Or the destination service used to accept both `"true"` and `true` but is now suddenly more strict.
- destination internally drops request - This is one of the more insidious failure modes because it is almost impossible to detect. This occurs when the destination service responds with a 2xx HTTP response code but does not actually process the message. Worse still, this issue is normally discovered days or weeks after it first occurs. The false-positive of a successful request being sent and the delay in detecting this issue makes it one of the more challenging failure modes to resolve.

## License

Licensed under either of
 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
