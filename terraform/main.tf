variable "project" {
  type    = string
  default = "SCORINGML"
}

variable "efs_mount_point" {
  type    = string
  default = "/usr/src/app/stupid_json"
}

provider "aws" {
  region = "eu-west-1"
}

resource "aws_cloudwatch_log_group" "feature_server" {
  name              = "/ecs/feature-server"
  retention_in_days = 1
}

resource "aws_iam_role" "ecs_execution_role" {
  name = "${var.project}-ecs-execution-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "ecs-tasks.amazonaws.com"
        }
      }
    ]
  })
}
resource "aws_iam_policy" "efs_policy" {
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow",
        Action = [
          "elasticfilesystem:ClientMount",
          "elasticfilesystem:ClientWrite",
          "elasticfilesystem:ClientRootAccess"
        ],
        Resource = aws_efs_file_system.my_efs.arn
      },
      {
        Effect = "Allow",
        Action = [
          "ec2:DescribeNetworkInterfaces",
          "ec2:DescribeInstances",
          "ec2:CreateNetworkInterface",
          "ec2:DeleteNetworkInterface",
          "ec2:ModifyNetworkInterfaceAttribute",
          "ec2:AssignPrivateIpAddresses",
          "ec2:UnassignPrivateIpAddresses"
        ],
        Resource = "*"
      }
    ]
  })
}

resource "aws_iam_role_policy_attachment" "ecs_execution_role_policy" {
  role       = aws_iam_role.ecs_execution_role.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role" "ecs_task_role" {
  name = "${var.project}-ecs-task-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "ecs-tasks.amazonaws.com"
        }
      }
    ]
  })
}

resource "aws_iam_role_policy_attachment" "ecs_task_tole_policy" {
  role       = aws_iam_role.ecs_task_role.name
  policy_arn = aws_iam_policy.efs_policy.arn
}

resource "aws_ecs_task_definition" "feature_server" {
  depends_on               = [null_resource.upload_image]
  family                   = "features-server"
  requires_compatibilities = ["FARGATE"]
  cpu                      = 256
  memory                   = 512
  network_mode             = "awsvpc"
  execution_role_arn       = aws_iam_role.ecs_execution_role.arn
  task_role_arn            = aws_iam_role.ecs_task_role.arn
  volume {
    name = "efs-volume"

    efs_volume_configuration {
      file_system_id = aws_efs_file_system.my_efs.id
    }
  }
  container_definitions = jsonencode(
    [
      {
        name  = "feature-server"
        image = "${aws_ecr_repository.ecr.repository_url}:${var.image_tag}"
        environment = [
          {
            name  = "FEATURES_PATH",
            value = var.efs_mount_point
          },
          {
            name  = "FEATURES_HOST"
            value = "0.0.0.0"
          },
          {
            name  = "ECS_TASK"
            value = "true"
          }
        ]
        portMappings = [
          {
            containerPort = 8080
            hostPort      = 8080
          }
        ]
        logConfiguration = {
          logDriver = "awslogs"
          options = {
            awslogs-group         = aws_cloudwatch_log_group.feature_server.name
            awslogs-region        = var.region
            awslogs-stream-prefix = "ecs"
          }
        }
        mountPoints = [
          {
            sourceVolume  = "efs-volume"
            containerPath = var.efs_mount_point
            readOnly      = false
          }
        ]
      }
    ]
  )

}

resource "aws_ecs_task_definition" "update_features" {
  depends_on               = [null_resource.upload_image_feats_update]
  family                   = "update-features"
  requires_compatibilities = ["FARGATE"]
  cpu                      = 4096
  memory                   = 8192
  network_mode             = "awsvpc"
  execution_role_arn       = aws_iam_role.ecs_execution_role.arn
  task_role_arn            = aws_iam_role.ecs_task_role.arn
  volume {
    name = "efs-volume"

    efs_volume_configuration {
      file_system_id = aws_efs_file_system.my_efs.id
    }
  }
  container_definitions = jsonencode(
    [
      {
        name  = "update-features"
        image = "${aws_ecr_repository.ecr_update_features.repository_url}:${var.image_tag}"
        environment = [
          {
            name  = "FEATURES_PATH",
            value = var.efs_mount_point
          },
          {
            name  = "ECS_TASK"
            value = "true"
          }

        ]
        logConfiguration = {
          logDriver = "awslogs"
          options = {
            awslogs-group         = aws_cloudwatch_log_group.feature_server.name
            awslogs-region        = var.region
            awslogs-stream-prefix = "ecs"
          }
        }
        mountPoints = [
          {
            sourceVolume  = "efs-volume"
            containerPath = var.efs_mount_point
            readOnly      = false
          }
        ]
      }
    ]
  )

}

resource "aws_ecs_service" "feature_server_service" {
  name            = "feature-server-service"
  depends_on      = [aws_vpc_endpoint.ecs, aws_vpc_endpoint.ecs-agent, aws_vpc_endpoint.ecs-telemetry, aws_vpc_endpoint.ecr_dkr]
  cluster         = aws_ecs_cluster.feature_server.id
  task_definition = aws_ecs_task_definition.feature_server.arn
  desired_count   = 4
  launch_type     = "FARGATE"

  load_balancer {
    target_group_arn = aws_lb_target_group.scoring_tg.arn
    container_name   = "feature-server"
    container_port   = 8080
  }

  network_configuration {
    subnets         = aws_subnet.private_subnets.*.id
    security_groups = [aws_security_group.features_server.id]
  }

  tags = {
    Project = var.project
  }
}

output "ecr_url" {
  value = aws_ecr_repository.ecr.repository_url
}
output "ecs_cluster" {
  value = aws_ecs_cluster.feature_server.name
}
output "ecs_td" {
  value = aws_ecs_task_definition.feature_server.arn
}
output "alb_dns" {
  value = aws_lb.load_balancer.dns_name
}
