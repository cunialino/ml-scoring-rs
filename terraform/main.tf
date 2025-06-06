variable "project" {
  type    = string
  default = "SCORINGML"
}

variable "efs_mount_point" {
  type    = string
  default = "/usr/src/app/stupid_json"
}

variable "docker_user" {
  type    = string
  default = "ecunial"
}

variable "docker_token" {
  type = string
}

provider "aws" {
  region = "eu-west-1"
}


# output "ecr_url" {
#   value = aws_ecr_repository.ecr.repository_url
# }
# output "ecs_cluster" {
#   value = aws_ecs_cluster.feature_server.name
# }
# output "ecs_td" {
#   value = aws_ecs_task_definition.feature_server.arn
# }
# output "alb_dns" {
#   value = aws_lb.load_balancer.dns_name
# }
