# soldr

A webhook delivery network

## Architecture

### Ingest

The ingest service uses the domain name to figure out the account and service. Example: acme-orders-239dsx.wh.soldr.dev

- The `wh` subdomain is what the ingest service listens on.
- The sub-subdomain contains tenant specific information. The first part is the account id. The second part is the service id. The last part is some random bits to prevent people from easily guessing valid domains.
   - The account and service moniker cannot contain hyphens.

The ingest service is designed to be very simple: it immediately saves the request to the queue. Currently, the queue is a postgres table. If that fails, it will log the request to stdout. The intention is that we _never_ dropa request. If the database is down, we can parse the logs to restore missing requests.

Immediately saving the request means we may save requests that are not for any specific tenant. We will delete these later. In the future, we may decide to keep a cached list of valid subdomains.

The ingest service is a separate package from the rest of the ecosystem. We do this so we can re-deploy the other parts with minimal fear of losing data.

### Processor

We use pg-boss to manage our postgres queue. The pg-boss workers will attempt to send a request that has not yet been sent. Each _attempt_ to send a request will record the response body and the time the request was made.

If a request is successfully sent, then we will mark that request as complete and no longer try and process it.

If a request fails, we will mark the next time the request should be tried. We will implement exponential backoff. Each service will have a max number of attempts. Once those attempts are exhausted, the processor will send an alert. We mark the state as alerted so we can later send a resolved email.

If a request does not relate to any active account, mark it as completed.

Key points:

- we must take great care to avoid sending the same request multiple times (unless we are trying to re-send a failed request)

#### Manual Intervention

The processor will expose an API that allows for a human to manually intervene. This allows us to present a UI to the user and they can re-send a request immediately. This will follow the rules above.

### Purge

Requests and attempts older than 30 days will be removed from the database. A pg-boss worker will remove these records in such a way that does not cause locking on the database. We may decide to add support for 90 days (or even longer) history later.

### Logical Schema

- accounts - each account (or tenant) has a unique id
- users - an account has one or more users that can access it
- services - an account has one or more services for their webhooks
- requests - each webhook request
- attempts - an attempt to send a request

### Metrics

We want to keep metrics around for a long time. HOW DO WE DO THIS!?!?!?

Key metrics:

- total number of requests
- total number of failed requests
- total number of requests that have been re-tried

### Website

- account signup page
- dashboard page that shows metrics for all webhooks for that account
- create service page
   - destination - must be a valid URL
- service details page that shows last 5 errors
   - also has full text search capability
   - can delete service
- request details page that shows the request and a list of attempts.
   - user can re-send the request
   - user can edit the request
- request edit page that allows the user to edit the request
- settings page for account settings
   - primary use is to set up alert email
