FROM microswarm-agent:dev AS microswarm
FROM python:3.13-alpine

COPY --from=microswarm /microswarm-agent /microswarm-agent
COPY --from=microswarm /microswarm-tools /microswarm-tools
COPY --from=microswarm /bin /bin
COPY --from=microswarm /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
USER 65532:65532
WORKDIR /session
ENTRYPOINT ["/microswarm-agent"]
