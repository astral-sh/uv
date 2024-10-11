use std::env;

use std::path::Path;

use anyhow::Result;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use insta::assert_json_snapshot;

use crate::workspace::{DiscoveryOptions, ProjectWorkspace};

async fn workspace_test(folder: &str) -> (ProjectWorkspace, String) {
    let root_dir = env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts")
        .join("workspaces");
    let project = ProjectWorkspace::discover(&root_dir.join(folder), &DiscoveryOptions::default())
        .await
        .unwrap();
    let root_escaped = regex::escape(root_dir.to_string_lossy().as_ref());
    (project, root_escaped)
}

async fn temporary_test(folder: &Path) -> (ProjectWorkspace, String) {
    let project = ProjectWorkspace::discover(folder, &DiscoveryOptions::default())
        .await
        .unwrap();
    let root_escaped = regex::escape(folder.to_string_lossy().as_ref());
    (project, root_escaped)
}

#[tokio::test]
async fn albatross_in_example() {
    let (project, root_escaped) = workspace_test("albatross-in-example/examples/bird-feeder").await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
    assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
    {
      "project_root": "[ROOT]/albatross-in-example/examples/bird-feeder",
      "project_name": "bird-feeder",
      "workspace": {
        "install_path": "[ROOT]/albatross-in-example/examples/bird-feeder",
        "packages": {
          "bird-feeder": {
            "root": "[ROOT]/albatross-in-example/examples/bird-feeder",
            "project": {
              "name": "bird-feeder",
              "version": "1.0.0",
              "requires-python": ">=3.12",
              "dependencies": [
                "anyio>=4.3.0,<5"
              ],
              "optional-dependencies": null
            },
            "pyproject_toml": "[PYPROJECT_TOML]"
          }
        },
        "sources": {},
        "pyproject_toml": {
          "project": {
            "name": "bird-feeder",
            "version": "1.0.0",
            "requires-python": ">=3.12",
            "dependencies": [
              "anyio>=4.3.0,<5"
            ],
            "optional-dependencies": null
          },
          "tool": null
        }
      }
    }
    "#);
    });
}

#[tokio::test]
async fn albatross_project_in_excluded() {
    let (project, root_escaped) =
        workspace_test("albatross-project-in-excluded/excluded/bird-feeder").await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
        {
          "project_root": "[ROOT]/albatross-project-in-excluded/excluded/bird-feeder",
          "project_name": "bird-feeder",
          "workspace": {
            "install_path": "[ROOT]/albatross-project-in-excluded/excluded/bird-feeder",
            "packages": {
              "bird-feeder": {
                "root": "[ROOT]/albatross-project-in-excluded/excluded/bird-feeder",
                "project": {
                  "name": "bird-feeder",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "anyio>=4.3.0,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "pyproject_toml": {
              "project": {
                "name": "bird-feeder",
                "version": "1.0.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "anyio>=4.3.0,<5"
                ],
                "optional-dependencies": null
              },
              "tool": null
            }
          }
        }
        "#);
    });
}

#[tokio::test]
async fn albatross_root_workspace() {
    let (project, root_escaped) = workspace_test("albatross-root-workspace").await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
        {
          "project_root": "[ROOT]/albatross-root-workspace",
          "project_name": "albatross",
          "workspace": {
            "install_path": "[ROOT]/albatross-root-workspace",
            "packages": {
              "albatross": {
                "root": "[ROOT]/albatross-root-workspace",
                "project": {
                  "name": "albatross",
                  "version": "0.1.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "bird-feeder",
                    "tqdm>=4,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              },
              "bird-feeder": {
                "root": "[ROOT]/albatross-root-workspace/packages/bird-feeder",
                "project": {
                  "name": "bird-feeder",
                  "version": "1.0.0",
                  "requires-python": ">=3.8",
                  "dependencies": [
                    "anyio>=4.3.0,<5",
                    "seeds"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              },
              "seeds": {
                "root": "[ROOT]/albatross-root-workspace/packages/seeds",
                "project": {
                  "name": "seeds",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "idna==3.6"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {
              "bird-feeder": [
                {
                  "workspace": true
                }
              ]
            },
            "pyproject_toml": {
              "project": {
                "name": "albatross",
                "version": "0.1.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "bird-feeder",
                  "tqdm>=4,<5"
                ],
                "optional-dependencies": null
              },
              "tool": {
                "uv": {
                  "sources": {
                    "bird-feeder": [
                      {
                        "workspace": true
                      }
                    ]
                  },
                  "workspace": {
                    "members": [
                      "packages/*"
                    ],
                    "exclude": null
                  },
                  "managed": null,
                  "package": null,
                  "dev-dependencies": null,
                  "environments": null,
                  "override-dependencies": null,
                  "constraint-dependencies": null
                }
              }
            }
          }
        }
        "#);
    });
}

#[tokio::test]
async fn albatross_virtual_workspace() {
    let (project, root_escaped) =
        workspace_test("albatross-virtual-workspace/packages/albatross").await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
        {
          "project_root": "[ROOT]/albatross-virtual-workspace/packages/albatross",
          "project_name": "albatross",
          "workspace": {
            "install_path": "[ROOT]/albatross-virtual-workspace",
            "packages": {
              "albatross": {
                "root": "[ROOT]/albatross-virtual-workspace/packages/albatross",
                "project": {
                  "name": "albatross",
                  "version": "0.1.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "bird-feeder",
                    "tqdm>=4,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              },
              "bird-feeder": {
                "root": "[ROOT]/albatross-virtual-workspace/packages/bird-feeder",
                "project": {
                  "name": "bird-feeder",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "anyio>=4.3.0,<5",
                    "seeds"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              },
              "seeds": {
                "root": "[ROOT]/albatross-virtual-workspace/packages/seeds",
                "project": {
                  "name": "seeds",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "idna==3.6"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "pyproject_toml": {
              "project": null,
              "tool": {
                "uv": {
                  "sources": null,
                  "workspace": {
                    "members": [
                      "packages/*"
                    ],
                    "exclude": null
                  },
                  "managed": null,
                  "package": null,
                  "dev-dependencies": null,
                  "environments": null,
                  "override-dependencies": null,
                  "constraint-dependencies": null
                }
              }
            }
          }
        }
        "#);
    });
}

#[tokio::test]
async fn albatross_just_project() {
    let (project, root_escaped) = workspace_test("albatross-just-project").await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
        {
          "project_root": "[ROOT]/albatross-just-project",
          "project_name": "albatross",
          "workspace": {
            "install_path": "[ROOT]/albatross-just-project",
            "packages": {
              "albatross": {
                "root": "[ROOT]/albatross-just-project",
                "project": {
                  "name": "albatross",
                  "version": "0.1.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "tqdm>=4,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "pyproject_toml": {
              "project": {
                "name": "albatross",
                "version": "0.1.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "tqdm>=4,<5"
                ],
                "optional-dependencies": null
              },
              "tool": null
            }
          }
        }
        "#);
    });
}
#[tokio::test]
async fn exclude_package() -> Result<()> {
    let root = tempfile::TempDir::new()?;
    let root = ChildPath::new(root.path());

    // Create the root.
    root.child("pyproject.toml").write_str(
        r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/*"]
            exclude = ["packages/bird-feeder"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
    )?;
    root.child("albatross").child("__init__.py").touch()?;

    // Create an included package (`seeds`).
    root.child("packages")
        .child("seeds")
        .child("pyproject.toml")
        .write_str(
            r#"
            [project]
            name = "seeds"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["idna==3.6"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
        )?;
    root.child("packages")
        .child("seeds")
        .child("seeds")
        .child("__init__.py")
        .touch()?;

    // Create an excluded package (`bird-feeder`).
    root.child("packages")
        .child("bird-feeder")
        .child("pyproject.toml")
        .write_str(
            r#"
            [project]
            name = "bird-feeder"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["anyio>=4.3.0,<5"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
        )?;
    root.child("packages")
        .child("bird-feeder")
        .child("bird_feeder")
        .child("__init__.py")
        .touch()?;

    let (project, root_escaped) = temporary_test(root.as_ref()).await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
        {
          "project_root": "[ROOT]",
          "project_name": "albatross",
          "workspace": {
            "install_path": "[ROOT]",
            "packages": {
              "albatross": {
                "root": "[ROOT]",
                "project": {
                  "name": "albatross",
                  "version": "0.1.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "tqdm>=4,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              },
              "seeds": {
                "root": "[ROOT]/packages/seeds",
                "project": {
                  "name": "seeds",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "idna==3.6"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "pyproject_toml": {
              "project": {
                "name": "albatross",
                "version": "0.1.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "tqdm>=4,<5"
                ],
                "optional-dependencies": null
              },
              "tool": {
                "uv": {
                  "sources": null,
                  "workspace": {
                    "members": [
                      "packages/*"
                    ],
                    "exclude": [
                      "packages/bird-feeder"
                    ]
                  },
                  "managed": null,
                  "package": null,
                  "dev-dependencies": null,
                  "environments": null,
                  "override-dependencies": null,
                  "constraint-dependencies": null
                }
              }
            }
          }
        }
        "#);
    });

    // Rewrite the members to both include and exclude `bird-feeder` by name.
    root.child("pyproject.toml").write_str(
        r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/seeds", "packages/bird-feeder"]
            exclude = ["packages/bird-feeder"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
    )?;

    // `bird-feeder` should still be excluded.
    let (project, root_escaped) = temporary_test(root.as_ref()).await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
        {
          "project_root": "[ROOT]",
          "project_name": "albatross",
          "workspace": {
            "install_path": "[ROOT]",
            "packages": {
              "albatross": {
                "root": "[ROOT]",
                "project": {
                  "name": "albatross",
                  "version": "0.1.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "tqdm>=4,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              },
              "seeds": {
                "root": "[ROOT]/packages/seeds",
                "project": {
                  "name": "seeds",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "idna==3.6"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "pyproject_toml": {
              "project": {
                "name": "albatross",
                "version": "0.1.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "tqdm>=4,<5"
                ],
                "optional-dependencies": null
              },
              "tool": {
                "uv": {
                  "sources": null,
                  "workspace": {
                    "members": [
                      "packages/seeds",
                      "packages/bird-feeder"
                    ],
                    "exclude": [
                      "packages/bird-feeder"
                    ]
                  },
                  "managed": null,
                  "package": null,
                  "dev-dependencies": null,
                  "environments": null,
                  "override-dependencies": null,
                  "constraint-dependencies": null
                }
              }
            }
          }
        }
        "#);
    });

    // Rewrite the exclusion to use the top-level directory (`packages`).
    root.child("pyproject.toml").write_str(
        r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/seeds", "packages/bird-feeder"]
            exclude = ["packages"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
    )?;

    // `bird-feeder` should now be included.
    let (project, root_escaped) = temporary_test(root.as_ref()).await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
        {
          "project_root": "[ROOT]",
          "project_name": "albatross",
          "workspace": {
            "install_path": "[ROOT]",
            "packages": {
              "albatross": {
                "root": "[ROOT]",
                "project": {
                  "name": "albatross",
                  "version": "0.1.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "tqdm>=4,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              },
              "bird-feeder": {
                "root": "[ROOT]/packages/bird-feeder",
                "project": {
                  "name": "bird-feeder",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "anyio>=4.3.0,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              },
              "seeds": {
                "root": "[ROOT]/packages/seeds",
                "project": {
                  "name": "seeds",
                  "version": "1.0.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "idna==3.6"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "pyproject_toml": {
              "project": {
                "name": "albatross",
                "version": "0.1.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "tqdm>=4,<5"
                ],
                "optional-dependencies": null
              },
              "tool": {
                "uv": {
                  "sources": null,
                  "workspace": {
                    "members": [
                      "packages/seeds",
                      "packages/bird-feeder"
                    ],
                    "exclude": [
                      "packages"
                    ]
                  },
                  "managed": null,
                  "package": null,
                  "dev-dependencies": null,
                  "environments": null,
                  "override-dependencies": null,
                  "constraint-dependencies": null
                }
              }
            }
          }
        }
        "#);
    });

    // Rewrite the exclusion to use the top-level directory with a glob (`packages/*`).
    root.child("pyproject.toml").write_str(
        r#"
            [project]
            name = "albatross"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["tqdm>=4,<5"]

            [tool.uv.workspace]
            members = ["packages/seeds", "packages/bird-feeder"]
            exclude = ["packages/*"]

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
            "#,
    )?;

    // `bird-feeder` and `seeds` should now be excluded.
    let (project, root_escaped) = temporary_test(root.as_ref()).await;
    let filters = vec![(root_escaped.as_str(), "[ROOT]")];
    insta::with_settings!({filters => filters}, {
        assert_json_snapshot!(
        project,
        {
            ".workspace.packages.*.pyproject_toml" => "[PYPROJECT_TOML]"
        },
        @r#"
        {
          "project_root": "[ROOT]",
          "project_name": "albatross",
          "workspace": {
            "install_path": "[ROOT]",
            "packages": {
              "albatross": {
                "root": "[ROOT]",
                "project": {
                  "name": "albatross",
                  "version": "0.1.0",
                  "requires-python": ">=3.12",
                  "dependencies": [
                    "tqdm>=4,<5"
                  ],
                  "optional-dependencies": null
                },
                "pyproject_toml": "[PYPROJECT_TOML]"
              }
            },
            "sources": {},
            "pyproject_toml": {
              "project": {
                "name": "albatross",
                "version": "0.1.0",
                "requires-python": ">=3.12",
                "dependencies": [
                  "tqdm>=4,<5"
                ],
                "optional-dependencies": null
              },
              "tool": {
                "uv": {
                  "sources": null,
                  "workspace": {
                    "members": [
                      "packages/seeds",
                      "packages/bird-feeder"
                    ],
                    "exclude": [
                      "packages/*"
                    ]
                  },
                  "managed": null,
                  "package": null,
                  "dev-dependencies": null,
                  "environments": null,
                  "override-dependencies": null,
                  "constraint-dependencies": null
                }
              }
            }
          }
        }
        "#);
    });

    Ok(())
}
