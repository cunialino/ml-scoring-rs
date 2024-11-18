# ML Scoring
Personal PoC of scoring Machine Learning models with Rust. 

The project is structured as follows:

- docker-compose.yaml: defines the services (features, score, grafana, prometheus)
- dockerfiles/: single docker file for rust services
- prometheus/: configuration files for prometheus
- grafana/: configuration files for grafana
- services/: Rust code 

    - feature_server/: code for a feature store. This is useful for batch features with infrequent updates, where something like redis would be an expansive overkill.

## TODO

Here we keep track of the next steps:

- [ ] add scoring service
    - [ ] choose model (in case create rust bindginds for such model when needed)

- [ ] improve prometheus metrics
