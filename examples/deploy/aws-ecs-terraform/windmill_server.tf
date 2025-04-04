resource "aws_cloudwatch_log_group" "windmill_cluster_windmill_server_log_group" {
  name = "/ecs/windmill-server"
}

resource "aws_ecs_task_definition" "windmill_cluster_windmill_server_td" {
  family             = "windmill-server"
  network_mode       = "awsvpc"
  execution_role_arn = data.aws_iam_role.ecs_task_execution_role.arn
  cpu                = 1024
  memory             = 1536
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }
  requires_compatibilities = ["EC2"]

  container_definitions = jsonencode([
    {
      name      = "windmill-server"
      image     = "ghcr.io/windmill-labs/windmill-ee:main"
      cpu       = 1024
      memory    = 1536
      essential = true
      portMappings = [
        {
          name          = "http"
          containerPort = 8000
          hostPort      = 8000
          protocol      = "tcp"
          appProtocol   = "http"
        }
      ]
      environment = [{
        name  = "JSON_FMT"
        value = "true"
        }, {
        name  = "DATABASE_URL"
        value = local.db_url
        }, {
        name  = "MODE"
        value = "server"
      }]
      healthCheck = {
        command  = ["CMD-SHELL", "curl -f http://localhost:8000/api/version || exit 1"]
        interval = 10
        timeout  = 5
        retries  = 5
      }
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.windmill_cluster_windmill_server_log_group.name
          "awslogs-region"        = data.aws_region.current.name
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])
}

resource "aws_ecs_service" "windmill_cluster_windmill_server_service" {
  name            = "windmill-server"
  cluster         = aws_ecs_cluster.windmill_cluster.id
  task_definition = aws_ecs_task_definition.windmill_cluster_windmill_server_td.arn
  desired_count   = 2

  network_configuration {
    subnets = [
      aws_subnet.windmill_cluster_subnet_private1.id,
      aws_subnet.windmill_cluster_subnet_private2.id,
    ]
    security_groups = [aws_security_group.windmill_cluster_sg.id]
  }

  force_new_deployment = true
  placement_constraints {
    type = "distinctInstance"
  }

  capacity_provider_strategy {
    capacity_provider = aws_ecs_capacity_provider.windmill_cluster_capacity_provider.name
    weight            = 100
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.windmill_cluster_windmill_server_tg.arn
    container_name   = "windmill-server"
    container_port   = 8000
  }

  depends_on = [aws_autoscaling_group.windmill_cluster_asg]
}
