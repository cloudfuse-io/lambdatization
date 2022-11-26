# lambdatization (l12n)

<h4 align="center">

[![Engines](https://github.com/cloudfuse-io/lambdatization/actions/workflows/engines.yaml/badge.svg?branch=main)](https://github.com/cloudfuse-io/lambdatization/actions/workflows/engines.yaml)
[![Style](https://github.com/cloudfuse-io/lambdatization/actions/workflows/style-check.yaml/badge.svg?branch=main)](https://github.com/cloudfuse-io/lambdatization/actions/workflows/style-check.yaml)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

</h4>

The goal of this project is to assess which query engines can
realistically run inside cloud functions (in particular AWS
Lambda) and have a first feeling about their performances in
this highly constrained environment.

## :chart_with_upwards_trend: Explore the results

We want to provide an accurate and interactive representation of our
experimental results. We believe that this is best achieved through
open interactive dasboards. This work is still work in progress, feel
free to play with it and give us your feedback!
- [NYC Taxi Parquet GROUP BY duration of various engines in AWS
  Lambda][datastudio-engine-duration]
- [AWS Lambda scale up duration by payload and function size][datastudio-scaling-duration]

[datastudio-engine-duration]: https://datastudio.google.com/reporting/c870737c-e8b6-467f-9860-8cd60c751f81
[datastudio-scaling-duration]: https://datastudio.google.com/reporting/0ffe5983-2dd2-4d53-9644-5154dc980784

## :hammer: Lambdatize yourself

### The `l12n-shell`
The `l12n-shell` provides a way to run all commands in an isolated Docker
environement. It is not strictly necessary, but simplifies the collaboration on
the project. To set it up:

- you must have Docker installed, it is the only dependency
- clone this repository
- `cd` into it
- run `L12N_BUILD=1 ./l12n-shell`
  - the `L12N_BUILD` environment variable indicates to the `l12n-shell` script
    that it needs to build the image
  - `./l12n-shell` looks for a `.env` file in the current directory to source
    environment variables from (see configuration section below)
  - the `./l12n-shell` without any argument runs an interactive bash terminal in
    the CLI container
  - `./l12n-shell cmd` and `echo "cmd" | ./l12n-shell` both run `cmd` in the
    `l12n-shell`

Note: the `l12n-shell` only works on amd64 for now.

### Configurations

`./l12n-shell` can be configured through environement variables or a `.env` in
the current directory:
- `L12N_PLUGINS` is a comma seprated list of plugins to activate
- `L12N_AWS_REGION` is the region where the stack should run

You can also provide the [usual][aws-cli-envvars] AWS variables:
- `AWS_PROFILE`
- `AWS_SHARED_CREDENTIALS_FILE`
- `AWS_ACCESS_KEY_ID`
- `AWS_SECRET_ACCESS_KEY`

[aws-cli-envvars]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-envvars.html

### The `l12n` CLI

Inside the `l12n-shell`, you can use the following commands:
- `l12n -h` to see all the available commands
- `l12n deploy -a` will run the terraform scripts and deploy the necessary
  resources (buckets, functions, roles...)
- `l12n destroy -a` to tear down the infrastructure and clean up your AWS
  account
- `l12n dockerized -e engine_name` runs a preconfigured query in the dockerized
  version of the specified engine locally. It requires the core module to be
  deployed to have access to the data
- `l12n run-lambda -e engine_name -c sql_query` runs the specified sql query on
  the given engine
  - you can also run pre-configured queries using the examples. Run `l12n -h` to
    see the list of examples.

###  About the stack

Infrastructure is managed by Terraform.

We use Terragrunt to:

- [DRY][wiki-dry] the Terraform config
- Manage dependencies between modules and allow a plugin based structure.

[wiki-dry]: https://en.wikipedia.org/wiki/Don%27t_repeat_yourself

### Contribute

- We follow the [conventional commits standard][conventionalcommits-v1] with
  [this][commitizen-list] list of _types_.
- We use the following linters:
  - [black](https://github.com/psf/black) for Python
  - [isort](https://pycqa.github.io/isort/) for Python imports
  - [yamllint](https://yamllint.readthedocs.io/en/stable/)
  - [markdownlint](https://github.com/markdownlint/markdownlint)

[conventionalcommits-v1]: https://www.conventionalcommits.org/en/v1.0.0/
[commitizen-list]: https://github.com/commitizen/conventional-commit-types/blob/master/index.json
