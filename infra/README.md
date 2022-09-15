# Infrastructure deployment scripts

Infrastructure is managed by Terraform.

## Terragrunt

We use Terragrunt to

- DRY the Terraform config
- Manage dependencies between modules

## Variables

Variables are sourced from the environment. If available, a `.env` file is
loaded from the directory in which the `l12n` script is executed.
