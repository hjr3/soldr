# TLS Certs

> [!CAUTION]
> These are intended for development only!

This directory contains TLS certs for localhost. These are used to make development easier.

## Making your own certificates

```
openssl req -x509 -out localhost.crt -keyout localhost.key \
  -newkey rsa:2048 -nodes -sha256 \
  -subj '/CN=localhost' -extensions EXT -config <( \
   printf "[dn]\nCN=localhost\n[req]\ndistinguished_name = dn\n[EXT]\nsubjectAltName=DNS:localhost\nkeyUsage=digitalSignature\nextendedKeyUsage=serverAuth")
```

[source](https://letsencrypt.org/docs/certificates-for-localhost/#making-and-trusting-your-own-certificates)
