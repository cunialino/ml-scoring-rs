resource "aws_efs_file_system" "my_efs" {
  creation_token = "${var.project}-efs"
  tags = {
    Name    = "${var.project}-efs"
    Project = var.project
  }
}

resource "aws_efs_mount_target" "my_efs_mount" {
  count          = length(var.azs)
  file_system_id = aws_efs_file_system.my_efs.id
  subnet_id      = aws_subnet.private_subnets[count.index].id
  security_groups = [
    aws_security_group.efs.id
  ]
}

resource "aws_ecs_cluster" "feature_server" {
  name = "feature-server-cluster"
  setting {
    name  = "containerInsights"
    value = "enabled"
  }
}
