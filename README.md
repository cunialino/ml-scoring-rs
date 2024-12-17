# ML Scoring
Personal PoC of scoring Machine Learning models with Rust. 

The project is structured as follows:

- docker-compose.yaml: defines the services (features, score, grafana, prometheus)
- dockerfiles/: single docker file for rust services
- prometheus/: configuration files for prometheus
- grafana/: configuration files for grafana
- services/: Rust code 

    - feature_server/: code for a feature store. This is useful for batch features with infrequent updates, where something like redis would be an expansive overkill.

## Notes

This is silly/stupid example. I had fun :D.

Open pain points: 

- Prometheus: i get wrong metrics from aws, this is due to alb redirecting requests to different tasks. I should implement a centralized prometheus in aws... but i can't be bothered/out of scope
- Prometheus: should implement some dynimic stuff in for ip. Rn I just get the alb dns out of terraform and manually replace it. Again, was not my goal to begin with
- EFS: I need a better way to create the features in efs, maybe tokio would be faster as rayon is more oriented to cpu bound tasks.

The project is a bit all over the place, but order was not the goal.

On the bright side: the whole thing does respond in less than 30ms (which was the objective)
