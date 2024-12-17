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
    Project                                     = var.project
    Name                                        = "${var.project}-public-subnet-${count.index}"
    "kubernetes.io/role/elb"                    = "1",
    "kubernetes.io/cluster/${var.cluster_name}" = "owned"
  }
}

resource "aws_subnet" "private_subnets" {
  count             = length(var.azs)
  vpc_id            = aws_vpc.scoring_vpc.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 8, 1 + length(var.azs) + count.index)
  availability_zone = element(var.azs, count.index)
  tags = {
    Project                                     = var.project
    Name                                        = "${var.project}-private-subnet-${count.index}"
    "kubernetes.io/role/internal-elb"           = "1",
    "kubernetes.io/cluster/${var.cluster_name}" = "owned"
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
    cidr_block = "0.0.0.0/0"
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

# resource "aws_eip" "nat" {
#   domain = "vpc"
# }
#
# resource "aws_nat_gateway" "nat" {
#   allocation_id = aws_eip.nat.id
#   subnet_id     = aws_subnet.public_subnets[0].id
#   tags = {
#     Name = "${var.project}-nat-gateway"
#   }
# }
#
# resource "aws_route_table" "private" {
#   vpc_id = aws_vpc.scoring_vpc.id
#
#   route {
#     cidr_block     = "0.0.0.0/0"
#     nat_gateway_id = aws_nat_gateway.nat.id
#   }
#
#   tags = {
#     Project = var.project
#     Name    = "${var.project}-private-rt"
#   }
# }

# resource "aws_route_table_association" "private" {
#   count          = length(aws_subnet.private_subnets)
#   subnet_id      = aws_subnet.private_subnets[count.index].id
#   route_table_id = aws_route_table.private.id
# }
#
resource "aws_security_group" "vpce" {
  name        = "${var.project}-vpce-sg"
  description = "Allow HTTPS from private subnets"
  vpc_id      = aws_vpc.scoring_vpc.id

  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = aws_subnet.private_subnets[*].cidr_block
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
    cidr_blocks = aws_subnet.private_subnets[*].cidr_block
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_vpc_endpoint" "ecr_api" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.ecr.api"
  vpc_endpoint_type   = "Interface"
  private_dns_enabled = true
  subnet_ids          = aws_subnet.private_subnets.*.id
  security_group_ids  = [aws_security_group.vpce.id]
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
  security_group_ids  = [aws_security_group.vpce.id]
  tags = {
    Name = "${var.project}-ecr-dkr-endpoint"
  }
}

resource "aws_vpc_endpoint" "elb_api" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.elasticloadbalancing"
  vpc_endpoint_type   = "Interface"
  subnet_ids          = aws_subnet.private_subnets.*.id
  security_group_ids  = [aws_security_group.vpce.id]
  private_dns_enabled = true

  tags = {
    Name = "${var.project}-elb-api-endpoint"
  }
}

resource "aws_vpc_endpoint" "ec2_api" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.ec2"
  vpc_endpoint_type   = "Interface"
  subnet_ids          = aws_subnet.private_subnets.*.id
  security_group_ids  = [aws_security_group.vpce.id]
  private_dns_enabled = true

  tags = {
    Name = "${var.project}-ec2-api-endpoint"
  }
}

resource "aws_vpc_endpoint" "sts_api" {
  vpc_id              = aws_vpc.scoring_vpc.id
  service_name        = "com.amazonaws.${var.region}.sts"
  vpc_endpoint_type   = "Interface"
  subnet_ids          = aws_subnet.private_subnets.*.id
  security_group_ids  = [aws_security_group.vpce.id]
  private_dns_enabled = true

  tags = {
    Name = "${var.project}-sts-api-endpoint"
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
# resource "aws_vpc_endpoint" "cloudwatch" {
#   service_name        = "com.amazonaws.${var.region}.logs"
#   vpc_id              = aws_vpc.scoring_vpc.id
#   private_dns_enabled = true
#   security_group_ids  = [aws_security_group.endpoints-sg.id]
#   vpc_endpoint_type   = "Interface"
#   subnet_ids          = aws_subnet.private_subnets.*.id
#   tags = {
#     Name = "${var.project}-cloudwatch-endpoint"
#   }
# }
resource "aws_security_group" "eks_sg" {
  name        = "${var.project}-eks-sg"
  description = "EKS ENI SG"
  vpc_id      = aws_vpc.scoring_vpc.id

  ingress {
    from_port   = 80
    to_port     = 80
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
