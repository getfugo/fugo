# Test certificates

The certificates of the tests of a proxy to an `https://` server (`tests/it/serve/proxy/https.rs`):
the test server answers as `localhost` with `localhost.pem` and `localhost.key`, and the proxy
trusts `ca.pem`, the authority that signed it, through `ServeOptions::proxy_roots`. ECDSA P-256,
valid from 2026-10-25 for 100 years. Made with OpenSSL 4.0 (the authority's key was not kept):

```sh
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout ca.key -out ca.pem \
  -days 36500 -subj "/CN=fugo test CA" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"
openssl req -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout localhost.key \
  -out localhost.csr -subj "/CN=localhost"
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n' > leaf.ext
openssl x509 -req -in localhost.csr -CA ca.pem -CAkey ca.key -set_serial 1 -out localhost.pem \
  -days 36500 -extfile leaf.ext
```
