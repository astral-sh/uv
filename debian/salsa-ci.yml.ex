# For more information on what jobs are run see:
# https://salsa.debian.org/salsa-ci-team/pipeline
#
# To enable the jobs, go to your repository (at salsa.debian.org)
# and click over Settings > CI/CD > Expand (in General pipelines).
# In "CI/CD configuration file" write debian/salsa-ci.yml and click
# in "Save Changes". The CI tests will run after the next commit.
---
include:
  - https://salsa.debian.org/salsa-ci-team/pipeline/raw/master/recipes/debian.yml
