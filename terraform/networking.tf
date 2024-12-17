variable "my_ip" {
  type    = string
  default = "0.0.0.0"
}
variable "cidr_block" {
  type    = string
  default = "0"
}
variable "region" {
  type    = string
  default = "eu-west-1"
}

variable "azs" {
  type    = list(string)
  default = ["eu-west-1a", "eu-west-1b"]
}

variable "vpc_cidr" {
  type    = string
  default = "10.0.0.0/16"
}

resource "aws_vpc" "scoring_vpc" {
  cidr_block           = var.vpc_cidr
  enable_dns_hostnames = true
  enable_dns_support   = true
  tags = {
    Project : var.project
    Name = "${var.project}-vpc"
  }
}

resource "aws_subnet" "public_subnets" {
  count             = length(var.azs)
  vpc_id            = aws_vpc.scoring_vpc.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 8, 1 + count.index)
  availability_zone = element(var.azs, count.index)
  tags = {
    Project : var.project
    Name = "${var.project}-public-subnet-${count.index}"
  }
}

resource "aws_subnet" "private_subnets" {
  count             = length(var.azs)
  vpc_id            = aws_vpc.scoring_vpc.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 8, 1 + length(var.azs) + count.index)
  availability_zone = element(var.azs, count.index)
  tags = {
    Project : var.project
    Name = "${var.project}-private-subnet-${count.index}"
  }
}

resource "aws_internet_gateway" "public" {
  vpc_id = aws_vpc.scoring_vpc.id
  tags = {
    Project : var.project
  }
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.scoring_vpc.id

  route {
    cidr_block = "${var.my_ip}/${var.cidr_block}"
    gateway_id = aws_internet_gateway.public.id
  }

  tags = {
    Project : var.project
  }
}

resource "aws_route_table_association" "public" {
  count          = length(aws_subnet.public_subnets)
  subnet_id      = aws_subnet.public_subnets[count.index].id
  route_table_id = aws_route_table.public.id
}

resource "aws_security_group" "features_server" {
  name        = "feature-server-sg"
  description = "Security group for features server"
  vpc_id      = aws_vpc.scoring_vpc.id

  ingress {
    from_port       = 8080
    to_port         = 8080
    protocol        = "tcp"
    security_groups = [aws_security_group.lb_sg.id]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_security_group" "endpoints-sg" {
  name        = "endppoints-sg"
  description = "Security group for vpc endpoints"
  vpc_id      = aws_vpc.scoring_vpc.id
  ingress {
    from_port       = 0
    to_port         = 0
    protocol        = "-1"
    security_groups = [aws_security_group.features_server.id]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_security_group" "efs" {
  name        = "efs-sg"
  description = "Security group for features server efs"
  vpc_id      = aws_vpc.scoring_vpc.id

  ingress {
    from_port       = 2049
    to_port         = 2049
    protocol        = "tcp"
    security_groups = [aws_security_group.features_server.id]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_lb" "load_balancer" {
  name               = "${var.project}-lb"
  internal           = false
  load_balancer_type = "application"
  security_groups    = [aws_security_group.lb_sg.id]
  subnets            = aws_subnet.public_subnets.*.id
}


resource "aws_security_group" "lb_sg" {
  name        = "load-balancer-sg"
  description = "Security group for features server"
  vpc_id      = aws_vpc.scoring_vpc.id

  ingress {
    from_port   = 8080
    to_port     = 8080
    protocol    = "tcp"
    cidr_blocks = ["${var.my_ip}/${var.cidr_block}"]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_lb_target_group" "scoring_tg" {
  name        = "${var.project}-tg"
  port        = 8080
  protocol    = "HTTP"
  vpc_id      = aws_vpc.scoring_vpc.id
  target_type = "ip"
  health_check {
    interval            = 5
    path                = "/health"
    protocol            = "HTTP"
    timeout             = 2
    healthy_threshold   = 2
    unhealthy_threshold = 5
  }
}

resource "aws_lb_listener" "http" {
  load_balancer_arn = aws_lb.load_balancer.arn
  port              = 8080
  protocol          = "HTTP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.scoring_tg.arn
  }
}

resource "aws_vpc_endpoint" "ecr_api" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.ecr.api"
  vpc_endpoint_type   = "Interface"
  private_dns_enabled = true
  subnet_ids          = aws_subnet.private_subnets.*.id
  security_group_ids  = [aws_security_group.endpoints-sg.id]
  tags = {
    Name = "${var.project}-ecr-api-endpoint"
  }
}
resource "aws_vpc_endpoint" "ecr_dkr" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.ecr.dkr"
  vpc_endpoint_type   = "Interface"
  private_dns_enabled = true
  subnet_ids          = aws_subnet.private_subnets.*.id
  security_group_ids  = [aws_security_group.endpoints-sg.id]
  tags = {
    Name = "${var.project}-ecr-dkr-endpoint"
  }
}
resource "aws_vpc_endpoint" "ecs-agent" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.ecs-agent"
  vpc_endpoint_type   = "Interface"
  private_dns_enabled = true
  subnet_ids          = aws_subnet.private_subnets.*.id
  security_group_ids  = [aws_security_group.endpoints-sg.id]
  tags = {
    Name = "${var.project}-ecs-agent-endpoint"
  }
}
resource "aws_vpc_endpoint" "ecs-telemetry" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.ecs-telemetry"
  vpc_endpoint_type   = "Interface"
  private_dns_enabled = true
  subnet_ids          = aws_subnet.private_subnets.*.id
  security_group_ids  = [aws_security_group.endpoints-sg.id]
  tags = {
    Name = "${var.project}-ecs-telemetry-endpoint"
  }
}
resource "aws_vpc_endpoint" "ecs" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.ecs"
  vpc_endpoint_type   = "Interface"
  private_dns_enabled = true
  security_group_ids  = [aws_security_group.endpoints-sg.id]
  tags = {
    Name = "${var.project}-ecs-endpoint"
  }
}
resource "aws_vpc_endpoint" "s3" {
  vpc_id          = aws_vpc.scoring_vpc.id
  service_name    = "com.amazonaws.${var.region}.s3"
  route_table_ids = [aws_vpc.scoring_vpc.default_route_table_id]
  tags = {
    Name = "${var.project}-s3-endpoint"
  }
}
resource "aws_vpc_endpoint" "cloudwatch" {
  service_name        = "com.amazonaws.${var.region}.logs"
  vpc_id              = aws_vpc.scoring_vpc.id
  private_dns_enabled = true
  security_group_ids  = [aws_security_group.endpoints-sg.id]
  vpc_endpoint_type   = "Interface"
  subnet_ids          = aws_subnet.private_subnets.*.id
  tags = {
    Name = "${var.project}-cloudwatch-endpoint"
  }
}
