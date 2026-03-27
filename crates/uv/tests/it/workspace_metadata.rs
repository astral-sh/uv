use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;

use uv_test::{copy_dir_ignore, uv_snapshot};

/// Test basic metadata output for a simple workspace with one member.
#[test]
fn workspace_metadata_simple() {
    let context = uv_test::test_context!("3.12");

    // Initialize a workspace with one member
    context.init().arg("foo").assert().success();

    let workspace = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/foo",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "foo",
          "path": "[TEMP_DIR]/foo",
          "id": "foo==0.1.0@virtual+[TEMP_DIR]/foo/"
        }
      ],
      "resolution": {
        "foo==0.1.0@virtual+[TEMP_DIR]/foo/": {
          "name": "foo",
          "version": "0.1.0",
          "source": {
            "virtual": "[TEMP_DIR]/foo/"
          },
          "kind": "package",
          "dependencies": []
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    "#
    );
}

/// Test metadata for a root workspace (workspace with a root package).
#[test]
fn workspace_metadata_root_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-root-workspace"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace",
          "id": "albatross==0.1.0@editable+[TEMP_DIR]/workspace/"
        },
        {
          "name": "bird-feeder",
          "path": "[TEMP_DIR]/workspace/packages/bird-feeder",
          "id": "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder"
        },
        {
          "name": "seeds",
          "path": "[TEMP_DIR]/workspace/packages/seeds",
          "id": "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds"
        }
      ],
      "resolution": {
        "albatross==0.1.0@editable+[TEMP_DIR]/workspace/": {
          "name": "albatross",
          "version": "0.1.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder"
            },
            {
              "id": "iniconfig==2.0.0@registry+https://pypi.org/simple"
            }
          ]
        },
        "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder": {
          "name": "bird-feeder",
          "version": "1.0.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/packages/bird-feeder"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "iniconfig==2.0.0@registry+https://pypi.org/simple"
            },
            {
              "id": "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds"
            }
          ]
        },
        "idna==3.6@registry+https://pypi.org/simple": {
          "name": "idna",
          "version": "3.6",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz",
            "hashes": {
              "sha256": "9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
            },
            "size": 175426,
            "upload_time": "2023-11-25T15:40:54.902Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl",
              "hashes": {
                "sha256": "c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
              },
              "size": 61567,
              "upload_time": "2023-11-25T15:40:52.604Z",
              "filename": "idna-3.6-py3-none-any.whl"
            }
          ]
        },
        "iniconfig==2.0.0@registry+https://pypi.org/simple": {
          "name": "iniconfig",
          "version": "2.0.0",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
            "hashes": {
              "sha256": "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
            },
            "size": 4646,
            "upload_time": "2023-01-07T11:08:11.254Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
              "hashes": {
                "sha256": "b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"
              },
              "size": 5892,
              "upload_time": "2023-01-07T11:08:09.864Z",
              "filename": "iniconfig-2.0.0-py3-none-any.whl"
            }
          ]
        },
        "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds": {
          "name": "seeds",
          "version": "1.0.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/packages/seeds"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "idna==3.6@registry+https://pypi.org/simple"
            }
          ]
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 5 packages in [TIME]
    "#
    );

    Ok(())
}

/// Test metadata for a virtual workspace (no root package).
#[test]
fn workspace_metadata_virtual_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-virtual-workspace"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace/packages/albatross",
          "id": "albatross==0.1.0@editable+[TEMP_DIR]/workspace/packages/albatross"
        },
        {
          "name": "bird-feeder",
          "path": "[TEMP_DIR]/workspace/packages/bird-feeder",
          "id": "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder"
        },
        {
          "name": "seeds",
          "path": "[TEMP_DIR]/workspace/packages/seeds",
          "id": "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds"
        }
      ],
      "resolution": {
        "albatross==0.1.0@editable+[TEMP_DIR]/workspace/packages/albatross": {
          "name": "albatross",
          "version": "0.1.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/packages/albatross"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder"
            },
            {
              "id": "iniconfig==2.0.0@registry+https://pypi.org/simple"
            }
          ]
        },
        "anyio==4.3.0@registry+https://pypi.org/simple": {
          "name": "anyio",
          "version": "4.3.0",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "idna==3.6@registry+https://pypi.org/simple"
            },
            {
              "id": "sniffio==1.3.1@registry+https://pypi.org/simple"
            }
          ],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz",
            "hashes": {
              "sha256": "f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6"
            },
            "size": 159642,
            "upload_time": "2024-02-19T08:36:28.641Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl",
              "hashes": {
                "sha256": "048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8"
              },
              "size": 85584,
              "upload_time": "2024-02-19T08:36:26.842Z",
              "filename": "anyio-4.3.0-py3-none-any.whl"
            }
          ]
        },
        "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder": {
          "name": "bird-feeder",
          "version": "1.0.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/packages/bird-feeder"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "anyio==4.3.0@registry+https://pypi.org/simple"
            },
            {
              "id": "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds"
            }
          ]
        },
        "idna==3.6@registry+https://pypi.org/simple": {
          "name": "idna",
          "version": "3.6",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz",
            "hashes": {
              "sha256": "9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
            },
            "size": 175426,
            "upload_time": "2023-11-25T15:40:54.902Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl",
              "hashes": {
                "sha256": "c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
              },
              "size": 61567,
              "upload_time": "2023-11-25T15:40:52.604Z",
              "filename": "idna-3.6-py3-none-any.whl"
            }
          ]
        },
        "iniconfig==2.0.0@registry+https://pypi.org/simple": {
          "name": "iniconfig",
          "version": "2.0.0",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
            "hashes": {
              "sha256": "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
            },
            "size": 4646,
            "upload_time": "2023-01-07T11:08:11.254Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
              "hashes": {
                "sha256": "b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"
              },
              "size": 5892,
              "upload_time": "2023-01-07T11:08:09.864Z",
              "filename": "iniconfig-2.0.0-py3-none-any.whl"
            }
          ]
        },
        "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds": {
          "name": "seeds",
          "version": "1.0.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/packages/seeds"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "idna==3.6@registry+https://pypi.org/simple"
            }
          ]
        },
        "sniffio==1.3.1@registry+https://pypi.org/simple": {
          "name": "sniffio",
          "version": "1.3.1",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz",
            "hashes": {
              "sha256": "f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc"
            },
            "size": 20372,
            "upload_time": "2024-02-25T23:20:04.057Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl",
              "hashes": {
                "sha256": "2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2"
              },
              "size": 10235,
              "upload_time": "2024-02-25T23:20:01.196Z",
              "filename": "sniffio-1.3.1-py3-none-any.whl"
            }
          ]
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 7 packages in [TIME]
    "#
    );

    Ok(())
}

/// Test metadata when run from a workspace member directory.
#[test]
fn workspace_metadata_from_member() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-root-workspace"),
        &workspace,
    )?;

    let member_dir = workspace.join("packages").join("bird-feeder");

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&member_dir), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace",
          "id": "albatross==0.1.0@editable+[TEMP_DIR]/workspace/"
        },
        {
          "name": "bird-feeder",
          "path": "[TEMP_DIR]/workspace/packages/bird-feeder",
          "id": "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder"
        },
        {
          "name": "seeds",
          "path": "[TEMP_DIR]/workspace/packages/seeds",
          "id": "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds"
        }
      ],
      "resolution": {
        "albatross==0.1.0@editable+[TEMP_DIR]/workspace/": {
          "name": "albatross",
          "version": "0.1.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder"
            },
            {
              "id": "iniconfig==2.0.0@registry+https://pypi.org/simple"
            }
          ]
        },
        "bird-feeder==1.0.0@editable+[TEMP_DIR]/workspace/packages/bird-feeder": {
          "name": "bird-feeder",
          "version": "1.0.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/packages/bird-feeder"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "iniconfig==2.0.0@registry+https://pypi.org/simple"
            },
            {
              "id": "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds"
            }
          ]
        },
        "idna==3.6@registry+https://pypi.org/simple": {
          "name": "idna",
          "version": "3.6",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz",
            "hashes": {
              "sha256": "9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
            },
            "size": 175426,
            "upload_time": "2023-11-25T15:40:54.902Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl",
              "hashes": {
                "sha256": "c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
              },
              "size": 61567,
              "upload_time": "2023-11-25T15:40:52.604Z",
              "filename": "idna-3.6-py3-none-any.whl"
            }
          ]
        },
        "iniconfig==2.0.0@registry+https://pypi.org/simple": {
          "name": "iniconfig",
          "version": "2.0.0",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
            "hashes": {
              "sha256": "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
            },
            "size": 4646,
            "upload_time": "2023-01-07T11:08:11.254Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
              "hashes": {
                "sha256": "b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"
              },
              "size": 5892,
              "upload_time": "2023-01-07T11:08:09.864Z",
              "filename": "iniconfig-2.0.0-py3-none-any.whl"
            }
          ]
        },
        "seeds==1.0.0@editable+[TEMP_DIR]/workspace/packages/seeds": {
          "name": "seeds",
          "version": "1.0.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/packages/seeds"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "idna==3.6@registry+https://pypi.org/simple"
            }
          ]
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 5 packages in [TIME]
    "#
    );

    Ok(())
}

/// Test metadata for a workspace with multiple packages.
#[test]
fn workspace_metadata_multiple_members() {
    let context = uv_test::test_context!("3.12");

    // Initialize workspace root
    context.init().arg("pkg-a").assert().success();

    let workspace_root = context.temp_dir.child("pkg-a");

    // Add more members
    context
        .init()
        .arg("pkg-b")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("pkg-c")
        .current_dir(&workspace_root)
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace_root), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/pkg-a",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "pkg-a",
          "path": "[TEMP_DIR]/pkg-a",
          "id": "pkg-a==0.1.0@virtual+[TEMP_DIR]/pkg-a/"
        },
        {
          "name": "pkg-b",
          "path": "[TEMP_DIR]/pkg-a/pkg-b",
          "id": "pkg-b==0.1.0@virtual+[TEMP_DIR]/pkg-a/pkg-b"
        },
        {
          "name": "pkg-c",
          "path": "[TEMP_DIR]/pkg-a/pkg-c",
          "id": "pkg-c==0.1.0@virtual+[TEMP_DIR]/pkg-a/pkg-c"
        }
      ],
      "resolution": {
        "pkg-a==0.1.0@virtual+[TEMP_DIR]/pkg-a/": {
          "name": "pkg-a",
          "version": "0.1.0",
          "source": {
            "virtual": "[TEMP_DIR]/pkg-a/"
          },
          "kind": "package",
          "dependencies": []
        },
        "pkg-b==0.1.0@virtual+[TEMP_DIR]/pkg-a/pkg-b": {
          "name": "pkg-b",
          "version": "0.1.0",
          "source": {
            "virtual": "[TEMP_DIR]/pkg-a/pkg-b"
          },
          "kind": "package",
          "dependencies": []
        },
        "pkg-c==0.1.0@virtual+[TEMP_DIR]/pkg-a/pkg-c": {
          "name": "pkg-c",
          "version": "0.1.0",
          "source": {
            "virtual": "[TEMP_DIR]/pkg-a/pkg-c"
          },
          "kind": "package",
          "dependencies": []
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    "#
    );
}

/// Test metadata for a single project (not a workspace).
#[test]
fn workspace_metadata_single_project() {
    let context = uv_test::test_context!("3.12");

    context.init().arg("my-project").assert().success();

    let project = context.temp_dir.child("my-project");

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&project), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/my-project",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "my-project",
          "path": "[TEMP_DIR]/my-project",
          "id": "my-project==0.1.0@virtual+[TEMP_DIR]/my-project/"
        }
      ],
      "resolution": {
        "my-project==0.1.0@virtual+[TEMP_DIR]/my-project/": {
          "name": "my-project",
          "version": "0.1.0",
          "source": {
            "virtual": "[TEMP_DIR]/my-project/"
          },
          "kind": "package",
          "dependencies": []
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    "#
    );
}

/// Test metadata with excluded packages.
#[test]
fn workspace_metadata_with_excluded() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-project-in-excluded"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace",
          "id": "albatross==0.1.0@editable+[TEMP_DIR]/workspace/"
        }
      ],
      "resolution": {
        "albatross==0.1.0@editable+[TEMP_DIR]/workspace/": {
          "name": "albatross",
          "version": "0.1.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "iniconfig==2.0.0@registry+https://pypi.org/simple"
            }
          ]
        },
        "iniconfig==2.0.0@registry+https://pypi.org/simple": {
          "name": "iniconfig",
          "version": "2.0.0",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
            "hashes": {
              "sha256": "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
            },
            "size": 4646,
            "upload_time": "2023-01-07T11:08:11.254Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
              "hashes": {
                "sha256": "b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"
              },
              "size": 5892,
              "upload_time": "2023-01-07T11:08:09.864Z",
              "filename": "iniconfig-2.0.0-py3-none-any.whl"
            }
          ]
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "#
    );

    Ok(())
}

/// Test metadata with excluded packages.
#[test]
fn workspace_metadata_group_only() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-groups-only"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "resolution": {
        "iniconfig==2.0.0@registry+https://pypi.org/simple": {
          "name": "iniconfig",
          "version": "2.0.0",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
            "hashes": {
              "sha256": "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
            },
            "size": 4646,
            "upload_time": "2023-01-07T11:08:11.254Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
              "hashes": {
                "sha256": "b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"
              },
              "size": 5892,
              "upload_time": "2023-01-07T11:08:09.864Z",
              "filename": "iniconfig-2.0.0-py3-none-any.whl"
            }
          ]
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 1 package in [TIME]
    "#
    );

    Ok(())
}

/// Test metadata error when not in a project.
#[test]
fn workspace_metadata_no_project() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.workspace_metadata(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    error: No `pyproject.toml` found in current directory or any parent directory
    "
    );
}

/// Test optional-dependencies, dependency-groups, and build-system
#[test]
fn workspace_metadata_various_dependency_rainbow() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-dependency-rainbow"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace",
          "id": "albatross==0.1.0@editable+[TEMP_DIR]/workspace/"
        }
      ],
      "resolution": {
        "albatross:dev==0.1.0@editable+[TEMP_DIR]/workspace/": {
          "name": "albatross",
          "version": "0.1.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/"
          },
          "kind": {
            "group": "dev"
          },
          "dependencies": [
            {
              "id": "idna==3.6@registry+https://pypi.org/simple"
            }
          ]
        },
        "albatross==0.1.0@editable+[TEMP_DIR]/workspace/": {
          "name": "albatross",
          "version": "0.1.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "iniconfig==2.0.0@registry+https://pypi.org/simple"
            }
          ],
          "optional_dependencies": [
            {
              "name": "io",
              "id": "albatross[io]==0.1.0@editable+[TEMP_DIR]/workspace/"
            }
          ],
          "dependency_groups": [
            {
              "name": "dev",
              "id": "albatross:dev==0.1.0@editable+[TEMP_DIR]/workspace/"
            }
          ]
        },
        "albatross[io]==0.1.0@editable+[TEMP_DIR]/workspace/": {
          "name": "albatross",
          "version": "0.1.0",
          "source": {
            "editable": "[TEMP_DIR]/workspace/"
          },
          "kind": {
            "extra": "io"
          },
          "dependencies": [
            {
              "id": "albatross==0.1.0@editable+[TEMP_DIR]/workspace/"
            },
            {
              "id": "anyio==4.3.0@registry+https://pypi.org/simple"
            }
          ]
        },
        "anyio==4.3.0@registry+https://pypi.org/simple": {
          "name": "anyio",
          "version": "4.3.0",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "idna==3.6@registry+https://pypi.org/simple"
            },
            {
              "id": "sniffio==1.3.1@registry+https://pypi.org/simple"
            }
          ],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz",
            "hashes": {
              "sha256": "f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6"
            },
            "size": 159642,
            "upload_time": "2024-02-19T08:36:28.641Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl",
              "hashes": {
                "sha256": "048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8"
              },
              "size": 85584,
              "upload_time": "2024-02-19T08:36:26.842Z",
              "filename": "anyio-4.3.0-py3-none-any.whl"
            }
          ]
        },
        "idna==3.6@registry+https://pypi.org/simple": {
          "name": "idna",
          "version": "3.6",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz",
            "hashes": {
              "sha256": "9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
            },
            "size": 175426,
            "upload_time": "2023-11-25T15:40:54.902Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl",
              "hashes": {
                "sha256": "c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
              },
              "size": 61567,
              "upload_time": "2023-11-25T15:40:52.604Z",
              "filename": "idna-3.6-py3-none-any.whl"
            }
          ]
        },
        "iniconfig==2.0.0@registry+https://pypi.org/simple": {
          "name": "iniconfig",
          "version": "2.0.0",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
            "hashes": {
              "sha256": "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
            },
            "size": 4646,
            "upload_time": "2023-01-07T11:08:11.254Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
              "hashes": {
                "sha256": "b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"
              },
              "size": 5892,
              "upload_time": "2023-01-07T11:08:09.864Z",
              "filename": "iniconfig-2.0.0-py3-none-any.whl"
            }
          ]
        },
        "sniffio==1.3.1@registry+https://pypi.org/simple": {
          "name": "sniffio",
          "version": "1.3.1",
          "source": {
            "registry": {
              "url": "https://pypi.org/simple"
            }
          },
          "kind": "package",
          "dependencies": [],
          "sdist": {
            "url": "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz",
            "hashes": {
              "sha256": "f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc"
            },
            "size": 20372,
            "upload_time": "2024-02-25T23:20:04.057Z"
          },
          "wheels": [
            {
              "url": "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl",
              "hashes": {
                "sha256": "2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2"
              },
              "size": 10235,
              "upload_time": "2024-02-25T23:20:01.196Z",
              "filename": "sniffio-1.3.1-py3-none-any.whl"
            }
          ]
        }
      }
    }

    ----- stderr -----
    warning: The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features workspace-metadata` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 5 packages in [TIME]
    "#
    );

    Ok(())
}
