#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use common::{uv_snapshot, TestContext};

mod common;

/// Lock a requirement from PyPI.
#[test]
fn lock_wheel_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["anyio==3.7.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 }]

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.8'"
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 }]
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Downloaded 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a requirement from PyPI.
#[test]
fn lock_multiple_wheels_registry() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("markupsafe==2.1.5")?;

    uv_snapshot!(context
        .compile()
        .arg("requirements.in")
        .arg("--generate-hashes")
        .arg("--unstable-uv-lock-file"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    # This file was autogenerated by uv via the following command:
    #    uv pip compile --cache-dir [CACHE_DIR] --exclude-newer 2024-03-25T00:00:00Z requirements.in --generate-hashes --unstable-uv-lock-file
    markupsafe==2.1.5 \
        --hash=sha256:00e046b6dd71aa03a41079792f8473dc494d564611a8f89bbbd7cb93295ebdcf \
        --hash=sha256:075202fa5b72c86ad32dc7d0b56024ebdbcf2048c0ba09f1cde31bfdd57bcfff \
        --hash=sha256:0e397ac966fdf721b2c528cf028494e86172b4feba51d65f81ffd65c63798f3f \
        --hash=sha256:17b950fccb810b3293638215058e432159d2b71005c74371d784862b7e4683f3 \
        --hash=sha256:1f3fbcb7ef1f16e48246f704ab79d79da8a46891e2da03f8783a5b6fa41a9532 \
        --hash=sha256:2174c595a0d73a3080ca3257b40096db99799265e1c27cc5a610743acd86d62f \
        --hash=sha256:2b7c57a4dfc4f16f7142221afe5ba4e093e09e728ca65c51f5620c9aaeb9a617 \
        --hash=sha256:2d2d793e36e230fd32babe143b04cec8a8b3eb8a3122d2aceb4a371e6b09b8df \
        --hash=sha256:30b600cf0a7ac9234b2638fbc0fb6158ba5bdcdf46aeb631ead21248b9affbc4 \
        --hash=sha256:397081c1a0bfb5124355710fe79478cdbeb39626492b15d399526ae53422b906 \
        --hash=sha256:3a57fdd7ce31c7ff06cdfbf31dafa96cc533c21e443d57f5b1ecc6cdc668ec7f \
        --hash=sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4 \
        --hash=sha256:3e53af139f8579a6d5f7b76549125f0d94d7e630761a2111bc431fd820e163b8 \
        --hash=sha256:4096e9de5c6fdf43fb4f04c26fb114f61ef0bf2e5604b6ee3019d51b69e8c371 \
        --hash=sha256:4275d846e41ecefa46e2015117a9f491e57a71ddd59bbead77e904dc02b1bed2 \
        --hash=sha256:4c31f53cdae6ecfa91a77820e8b151dba54ab528ba65dfd235c80b086d68a465 \
        --hash=sha256:4f11aa001c540f62c6166c7726f71f7573b52c68c31f014c25cc7901deea0b52 \
        --hash=sha256:5049256f536511ee3f7e1b3f87d1d1209d327e818e6ae1365e8653d7e3abb6a6 \
        --hash=sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169 \
        --hash=sha256:598e3276b64aff0e7b3451b72e94fa3c238d452e7ddcd893c3ab324717456bad \
        --hash=sha256:5b7b716f97b52c5a14bffdf688f971b2d5ef4029127f1ad7a513973cfd818df2 \
        --hash=sha256:5dedb4db619ba5a2787a94d877bc8ffc0566f92a01c0ef214865e54ecc9ee5e0 \
        --hash=sha256:619bc166c4f2de5caa5a633b8b7326fbe98e0ccbfacabd87268a2b15ff73a029 \
        --hash=sha256:629ddd2ca402ae6dbedfceeba9c46d5f7b2a61d9749597d4307f943ef198fc1f \
        --hash=sha256:656f7526c69fac7f600bd1f400991cc282b417d17539a1b228617081106feb4a \
        --hash=sha256:6ec585f69cec0aa07d945b20805be741395e28ac1627333b1c5b0105962ffced \
        --hash=sha256:72b6be590cc35924b02c78ef34b467da4ba07e4e0f0454a2c5907f473fc50ce5 \
        --hash=sha256:7502934a33b54030eaf1194c21c692a534196063db72176b0c4028e140f8f32c \
        --hash=sha256:7a68b554d356a91cce1236aa7682dc01df0edba8d043fd1ce607c49dd3c1edcf \
        --hash=sha256:7b2e5a267c855eea6b4283940daa6e88a285f5f2a67f2220203786dfa59b37e9 \
        --hash=sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb \
        --hash=sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad \
        --hash=sha256:8dd717634f5a044f860435c1d8c16a270ddf0ef8588d4887037c5028b859b0c3 \
        --hash=sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1 \
        --hash=sha256:97cafb1f3cbcd3fd2b6fbfb99ae11cdb14deea0736fc2b0952ee177f2b813a46 \
        --hash=sha256:a17a92de5231666cfbe003f0e4b9b3a7ae3afb1ec2845aadc2bacc93ff85febc \
        --hash=sha256:a549b9c31bec33820e885335b451286e2969a2d9e24879f83fe904a5ce59d70a \
        --hash=sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee \
        --hash=sha256:ae2ad8ae6ebee9d2d94b17fb62763125f3f374c25618198f40cbb8b525411900 \
        --hash=sha256:b91c037585eba9095565a3556f611e3cbfaa42ca1e865f7b8015fe5c7336d5a5 \
        --hash=sha256:bc1667f8b83f48511b94671e0e441401371dfd0f0a795c7daa4a3cd1dde55bea \
        --hash=sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f \
        --hash=sha256:bf50cd79a75d181c9181df03572cdce0fbb75cc353bc350712073108cba98de5 \
        --hash=sha256:bff1b4290a66b490a2f4719358c0cdcd9bafb6b8f061e45c7a2460866bf50c2e \
        --hash=sha256:c061bb86a71b42465156a3ee7bd58c8c2ceacdbeb95d05a99893e08b8467359a \
        --hash=sha256:c8b29db45f8fe46ad280a7294f5c3ec36dbac9491f2d1c17345be8e69cc5928f \
        --hash=sha256:ce409136744f6521e39fd8e2a24c53fa18ad67aa5bc7c2cf83645cce5b5c4e50 \
        --hash=sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a \
        --hash=sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b \
        --hash=sha256:d9fad5155d72433c921b782e58892377c44bd6252b5af2f67f16b194987338a4 \
        --hash=sha256:daa4ee5a243f0f20d528d939d06670a298dd39b1ad5f8a72a4275124a7819eff \
        --hash=sha256:db0b55e0f3cc0be60c1f19efdde9a637c32740486004f20d1cff53c3c0ece4d2 \
        --hash=sha256:e61659ba32cf2cf1481e575d0462554625196a1f2fc06a1c777d3f48e8865d46 \
        --hash=sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b \
        --hash=sha256:ec6a563cff360b50eed26f13adc43e61bc0c04d94b8be985e6fb24b81f6dcfdf \
        --hash=sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5 \
        --hash=sha256:fa173ec60341d6bb97a89f5ea19c85c5643c1e7dedebc22f5181eb73573142c5 \
        --hash=sha256:fa9db3f79de01457b03d4f01b34cf91bc0048eb2c3846ff26f66687c2f6d16ab \
        --hash=sha256:fce659a462a1be54d2ffcacea5e3ba2d74daa74f30f5f143fe0c58636e355fdd \
        --hash=sha256:ffee1f21e5ef0d712f9033568f8344d5da8cc2869dbd08d87c84656e6a2d2f68
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    assert_snapshot!(
        lock, @r###"
    version = 1

    [[distribution]]
    name = "markupsafe"
    version = "2.1.5"
    source = "registry+https://pypi.org/simple"
    sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
    wheels = [
    	{ url = "https://files.pythonhosted.org/packages/e4/54/ad5eb37bf9d51800010a74e4665425831a9db4e7c4e0fde4352e391e808e/MarkupSafe-2.1.5-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:a17a92de5231666cfbe003f0e4b9b3a7ae3afb1ec2845aadc2bacc93ff85febc", size = 18206 },
    	{ url = "https://files.pythonhosted.org/packages/6a/4a/a4d49415e600bacae038c67f9fecc1d5433b9d3c71a4de6f33537b89654c/MarkupSafe-2.1.5-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:72b6be590cc35924b02c78ef34b467da4ba07e4e0f0454a2c5907f473fc50ce5", size = 14079 },
    	{ url = "https://files.pythonhosted.org/packages/0a/7b/85681ae3c33c385b10ac0f8dd025c30af83c78cec1c37a6aa3b55e67f5ec/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e61659ba32cf2cf1481e575d0462554625196a1f2fc06a1c777d3f48e8865d46", size = 26620 },
    	{ url = "https://files.pythonhosted.org/packages/7c/52/2b1b570f6b8b803cef5ac28fdf78c0da318916c7d2fe9402a84d591b394c/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:2174c595a0d73a3080ca3257b40096db99799265e1c27cc5a610743acd86d62f", size = 25818 },
    	{ url = "https://files.pythonhosted.org/packages/29/fe/a36ba8c7ca55621620b2d7c585313efd10729e63ef81e4e61f52330da781/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ae2ad8ae6ebee9d2d94b17fb62763125f3f374c25618198f40cbb8b525411900", size = 25493 },
    	{ url = "https://files.pythonhosted.org/packages/60/ae/9c60231cdfda003434e8bd27282b1f4e197ad5a710c14bee8bea8a9ca4f0/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:075202fa5b72c86ad32dc7d0b56024ebdbcf2048c0ba09f1cde31bfdd57bcfff", size = 30630 },
    	{ url = "https://files.pythonhosted.org/packages/65/dc/1510be4d179869f5dafe071aecb3f1f41b45d37c02329dfba01ff59e5ac5/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_i686.whl", hash = "sha256:598e3276b64aff0e7b3451b72e94fa3c238d452e7ddcd893c3ab324717456bad", size = 29745 },
    	{ url = "https://files.pythonhosted.org/packages/30/39/8d845dd7d0b0613d86e0ef89549bfb5f61ed781f59af45fc96496e897f3a/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:fce659a462a1be54d2ffcacea5e3ba2d74daa74f30f5f143fe0c58636e355fdd", size = 30021 },
    	{ url = "https://files.pythonhosted.org/packages/c7/5c/356a6f62e4f3c5fbf2602b4771376af22a3b16efa74eb8716fb4e328e01e/MarkupSafe-2.1.5-cp310-cp310-win32.whl", hash = "sha256:d9fad5155d72433c921b782e58892377c44bd6252b5af2f67f16b194987338a4", size = 16659 },
    	{ url = "https://files.pythonhosted.org/packages/69/48/acbf292615c65f0604a0c6fc402ce6d8c991276e16c80c46a8f758fbd30c/MarkupSafe-2.1.5-cp310-cp310-win_amd64.whl", hash = "sha256:bf50cd79a75d181c9181df03572cdce0fbb75cc353bc350712073108cba98de5", size = 17213 },
    	{ url = "https://files.pythonhosted.org/packages/11/e7/291e55127bb2ae67c64d66cef01432b5933859dfb7d6949daa721b89d0b3/MarkupSafe-2.1.5-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:629ddd2ca402ae6dbedfceeba9c46d5f7b2a61d9749597d4307f943ef198fc1f", size = 18219 },
    	{ url = "https://files.pythonhosted.org/packages/6b/cb/aed7a284c00dfa7c0682d14df85ad4955a350a21d2e3b06d8240497359bf/MarkupSafe-2.1.5-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:5b7b716f97b52c5a14bffdf688f971b2d5ef4029127f1ad7a513973cfd818df2", size = 14098 },
    	{ url = "https://files.pythonhosted.org/packages/1c/cf/35fe557e53709e93feb65575c93927942087e9b97213eabc3fe9d5b25a55/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:6ec585f69cec0aa07d945b20805be741395e28ac1627333b1c5b0105962ffced", size = 29014 },
    	{ url = "https://files.pythonhosted.org/packages/97/18/c30da5e7a0e7f4603abfc6780574131221d9148f323752c2755d48abad30/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b91c037585eba9095565a3556f611e3cbfaa42ca1e865f7b8015fe5c7336d5a5", size = 28220 },
    	{ url = "https://files.pythonhosted.org/packages/0c/40/2e73e7d532d030b1e41180807a80d564eda53babaf04d65e15c1cf897e40/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:7502934a33b54030eaf1194c21c692a534196063db72176b0c4028e140f8f32c", size = 27756 },
    	{ url = "https://files.pythonhosted.org/packages/18/46/5dca760547e8c59c5311b332f70605d24c99d1303dd9a6e1fc3ed0d73561/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:0e397ac966fdf721b2c528cf028494e86172b4feba51d65f81ffd65c63798f3f", size = 33988 },
    	{ url = "https://files.pythonhosted.org/packages/6d/c5/27febe918ac36397919cd4a67d5579cbbfa8da027fa1238af6285bb368ea/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:c061bb86a71b42465156a3ee7bd58c8c2ceacdbeb95d05a99893e08b8467359a", size = 32718 },
    	{ url = "https://files.pythonhosted.org/packages/f8/81/56e567126a2c2bc2684d6391332e357589a96a76cb9f8e5052d85cb0ead8/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:3a57fdd7ce31c7ff06cdfbf31dafa96cc533c21e443d57f5b1ecc6cdc668ec7f", size = 33317 },
    	{ url = "https://files.pythonhosted.org/packages/00/0b/23f4b2470accb53285c613a3ab9ec19dc944eaf53592cb6d9e2af8aa24cc/MarkupSafe-2.1.5-cp311-cp311-win32.whl", hash = "sha256:397081c1a0bfb5124355710fe79478cdbeb39626492b15d399526ae53422b906", size = 16670 },
    	{ url = "https://files.pythonhosted.org/packages/b7/a2/c78a06a9ec6d04b3445a949615c4c7ed86a0b2eb68e44e7541b9d57067cc/MarkupSafe-2.1.5-cp311-cp311-win_amd64.whl", hash = "sha256:2b7c57a4dfc4f16f7142221afe5ba4e093e09e728ca65c51f5620c9aaeb9a617", size = 17224 },
    	{ url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
    	{ url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
    	{ url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
    	{ url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
    	{ url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
    	{ url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
    	{ url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
    	{ url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
    	{ url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
    	{ url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
    	{ url = "https://files.pythonhosted.org/packages/a7/88/a940e11827ea1c136a34eca862486178294ae841164475b9ab216b80eb8e/MarkupSafe-2.1.5-cp37-cp37m-macosx_10_9_x86_64.whl", hash = "sha256:c8b29db45f8fe46ad280a7294f5c3ec36dbac9491f2d1c17345be8e69cc5928f", size = 13982 },
    	{ url = "https://files.pythonhosted.org/packages/cb/06/0d28bd178db529c5ac762a625c335a9168a7a23f280b4db9c95e97046145/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ec6a563cff360b50eed26f13adc43e61bc0c04d94b8be985e6fb24b81f6dcfdf", size = 26335 },
    	{ url = "https://files.pythonhosted.org/packages/4a/1d/c4f5016f87ced614eacc7d5fb85b25bcc0ff53e8f058d069fc8cbfdc3c7a/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a549b9c31bec33820e885335b451286e2969a2d9e24879f83fe904a5ce59d70a", size = 25557 },
    	{ url = "https://files.pythonhosted.org/packages/b3/fb/c18b8c9fbe69e347fdbf782c6478f1bc77f19a830588daa224236678339b/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4f11aa001c540f62c6166c7726f71f7573b52c68c31f014c25cc7901deea0b52", size = 25245 },
    	{ url = "https://files.pythonhosted.org/packages/2f/69/30d29adcf9d1d931c75001dd85001adad7374381c9c2086154d9f6445be6/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_aarch64.whl", hash = "sha256:7b2e5a267c855eea6b4283940daa6e88a285f5f2a67f2220203786dfa59b37e9", size = 31013 },
    	{ url = "https://files.pythonhosted.org/packages/3a/03/63498d05bd54278b6ca340099e5b52ffb9cdf2ee4f2d9b98246337e21689/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_i686.whl", hash = "sha256:2d2d793e36e230fd32babe143b04cec8a8b3eb8a3122d2aceb4a371e6b09b8df", size = 30178 },
    	{ url = "https://files.pythonhosted.org/packages/68/79/11b4fe15124692f8673b603433e47abca199a08ecd2a4851bfbdc97dc62d/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_x86_64.whl", hash = "sha256:ce409136744f6521e39fd8e2a24c53fa18ad67aa5bc7c2cf83645cce5b5c4e50", size = 30429 },
    	{ url = "https://files.pythonhosted.org/packages/ed/88/408bdbf292eb86f03201c17489acafae8358ba4e120d92358308c15cea7c/MarkupSafe-2.1.5-cp37-cp37m-win32.whl", hash = "sha256:4096e9de5c6fdf43fb4f04c26fb114f61ef0bf2e5604b6ee3019d51b69e8c371", size = 16633 },
    	{ url = "https://files.pythonhosted.org/packages/6c/4c/3577a52eea1880538c435176bc85e5b3379b7ab442327ccd82118550758f/MarkupSafe-2.1.5-cp37-cp37m-win_amd64.whl", hash = "sha256:4275d846e41ecefa46e2015117a9f491e57a71ddd59bbead77e904dc02b1bed2", size = 17215 },
    	{ url = "https://files.pythonhosted.org/packages/f8/ff/2c942a82c35a49df5de3a630ce0a8456ac2969691b230e530ac12314364c/MarkupSafe-2.1.5-cp38-cp38-macosx_10_9_universal2.whl", hash = "sha256:656f7526c69fac7f600bd1f400991cc282b417d17539a1b228617081106feb4a", size = 18192 },
    	{ url = "https://files.pythonhosted.org/packages/4f/14/6f294b9c4f969d0c801a4615e221c1e084722ea6114ab2114189c5b8cbe0/MarkupSafe-2.1.5-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:97cafb1f3cbcd3fd2b6fbfb99ae11cdb14deea0736fc2b0952ee177f2b813a46", size = 14072 },
    	{ url = "https://files.pythonhosted.org/packages/81/d4/fd74714ed30a1dedd0b82427c02fa4deec64f173831ec716da11c51a50aa/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:1f3fbcb7ef1f16e48246f704ab79d79da8a46891e2da03f8783a5b6fa41a9532", size = 26928 },
    	{ url = "https://files.pythonhosted.org/packages/c7/bd/50319665ce81bb10e90d1cf76f9e1aa269ea6f7fa30ab4521f14d122a3df/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:fa9db3f79de01457b03d4f01b34cf91bc0048eb2c3846ff26f66687c2f6d16ab", size = 26106 },
    	{ url = "https://files.pythonhosted.org/packages/4c/6f/f2b0f675635b05f6afd5ea03c094557bdb8622fa8e673387444fe8d8e787/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ffee1f21e5ef0d712f9033568f8344d5da8cc2869dbd08d87c84656e6a2d2f68", size = 25781 },
    	{ url = "https://files.pythonhosted.org/packages/51/e0/393467cf899b34a9d3678e78961c2c8cdf49fb902a959ba54ece01273fb1/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_aarch64.whl", hash = "sha256:5dedb4db619ba5a2787a94d877bc8ffc0566f92a01c0ef214865e54ecc9ee5e0", size = 30518 },
    	{ url = "https://files.pythonhosted.org/packages/f6/02/5437e2ad33047290dafced9df741d9efc3e716b75583bbd73a9984f1b6f7/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_i686.whl", hash = "sha256:30b600cf0a7ac9234b2638fbc0fb6158ba5bdcdf46aeb631ead21248b9affbc4", size = 29669 },
    	{ url = "https://files.pythonhosted.org/packages/0e/7d/968284145ffd9d726183ed6237c77938c021abacde4e073020f920e060b2/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_x86_64.whl", hash = "sha256:8dd717634f5a044f860435c1d8c16a270ddf0ef8588d4887037c5028b859b0c3", size = 29933 },
    	{ url = "https://files.pythonhosted.org/packages/bf/f3/ecb00fc8ab02b7beae8699f34db9357ae49d9f21d4d3de6f305f34fa949e/MarkupSafe-2.1.5-cp38-cp38-win32.whl", hash = "sha256:daa4ee5a243f0f20d528d939d06670a298dd39b1ad5f8a72a4275124a7819eff", size = 16656 },
    	{ url = "https://files.pythonhosted.org/packages/92/21/357205f03514a49b293e214ac39de01fadd0970a6e05e4bf1ddd0ffd0881/MarkupSafe-2.1.5-cp38-cp38-win_amd64.whl", hash = "sha256:619bc166c4f2de5caa5a633b8b7326fbe98e0ccbfacabd87268a2b15ff73a029", size = 17206 },
    	{ url = "https://files.pythonhosted.org/packages/0f/31/780bb297db036ba7b7bbede5e1d7f1e14d704ad4beb3ce53fb495d22bc62/MarkupSafe-2.1.5-cp39-cp39-macosx_10_9_universal2.whl", hash = "sha256:7a68b554d356a91cce1236aa7682dc01df0edba8d043fd1ce607c49dd3c1edcf", size = 18193 },
    	{ url = "https://files.pythonhosted.org/packages/6c/77/d77701bbef72892affe060cdacb7a2ed7fd68dae3b477a8642f15ad3b132/MarkupSafe-2.1.5-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:db0b55e0f3cc0be60c1f19efdde9a637c32740486004f20d1cff53c3c0ece4d2", size = 14073 },
    	{ url = "https://files.pythonhosted.org/packages/d9/a7/1e558b4f78454c8a3a0199292d96159eb4d091f983bc35ef258314fe7269/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:3e53af139f8579a6d5f7b76549125f0d94d7e630761a2111bc431fd820e163b8", size = 26486 },
    	{ url = "https://files.pythonhosted.org/packages/5f/5a/360da85076688755ea0cceb92472923086993e86b5613bbae9fbc14136b0/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:17b950fccb810b3293638215058e432159d2b71005c74371d784862b7e4683f3", size = 25685 },
    	{ url = "https://files.pythonhosted.org/packages/6a/18/ae5a258e3401f9b8312f92b028c54d7026a97ec3ab20bfaddbdfa7d8cce8/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4c31f53cdae6ecfa91a77820e8b151dba54ab528ba65dfd235c80b086d68a465", size = 25338 },
    	{ url = "https://files.pythonhosted.org/packages/0b/cc/48206bd61c5b9d0129f4d75243b156929b04c94c09041321456fd06a876d/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_aarch64.whl", hash = "sha256:bff1b4290a66b490a2f4719358c0cdcd9bafb6b8f061e45c7a2460866bf50c2e", size = 30439 },
    	{ url = "https://files.pythonhosted.org/packages/d1/06/a41c112ab9ffdeeb5f77bc3e331fdadf97fa65e52e44ba31880f4e7f983c/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_i686.whl", hash = "sha256:bc1667f8b83f48511b94671e0e441401371dfd0f0a795c7daa4a3cd1dde55bea", size = 29531 },
    	{ url = "https://files.pythonhosted.org/packages/02/8c/ab9a463301a50dab04d5472e998acbd4080597abc048166ded5c7aa768c8/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:5049256f536511ee3f7e1b3f87d1d1209d327e818e6ae1365e8653d7e3abb6a6", size = 29823 },
    	{ url = "https://files.pythonhosted.org/packages/bc/29/9bc18da763496b055d8e98ce476c8e718dcfd78157e17f555ce6dd7d0895/MarkupSafe-2.1.5-cp39-cp39-win32.whl", hash = "sha256:00e046b6dd71aa03a41079792f8473dc494d564611a8f89bbbd7cb93295ebdcf", size = 16658 },
    	{ url = "https://files.pythonhosted.org/packages/f6/f8/4da07de16f10551ca1f640c92b5f316f9394088b183c6a57183df6de5ae4/MarkupSafe-2.1.5-cp39-cp39-win_amd64.whl", hash = "sha256:fa173ec60341d6bb97a89f5ea19c85c5643c1e7dedebc22f5181eb73573142c5", size = 17211 }
    ]
    "###
    );

    // Install from the lockfile.
    uv_snapshot!(context.install().arg("--unstable-uv-lock-file").arg("markupsafe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.5
    "###);

    Ok(())
}

/// Lock a requirement from PyPI.
#[test]
fn lock_sdist_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["source-distribution==0.0.1"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock_without_exclude_newer(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "source-distribution"
        version = "0.0.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "source-distribution"
        version = "0.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz", hash = "sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106", size = 2157 }
        "###
        );
    });

    Ok(())
}

/// Lock a Git requirement.
#[test]
fn lock_sdist_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
        sdist = { url = "https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    Ok(())
}

/// Lock a requirement from a direct URL to a wheel.
#[test]
fn lock_wheel_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["anyio @ https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"
        wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 }]
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0 (from https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl)
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a requirement from a direct URL to a source distribution.
#[test]
fn lock_sdist_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6" }

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 }]
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Downloaded 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0 (from https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz)
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a project with an extra. When resolving, all extras should be included.
#[test]
fn lock_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["anyio==3.7.0"]

        [project.optional-dependencies]
        test = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 }]

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        extra = "test"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.8'"
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 }]
        "###
        );
    });

    // Install the base dependencies from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Downloaded 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    // Install the extras from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("test"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// Respect the locked version in an existing lockfile.
#[test]
fn lock_preference() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["iniconfig<2"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "iniconfig"
        version = "1.1.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "iniconfig"
        version = "1.1.1"
        source = "registry+https://pypi.org/simple"
        "###
        );
    });

    // Modify the `pyproject.toml` to loosen the requirement.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Ensure that the locked version is still respected.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "iniconfig"
        version = "1.1.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "iniconfig"
        version = "1.1.1"
        source = "registry+https://pypi.org/simple"
        "###
        );
    });

    // Run with `--upgrade`; ensure that `iniconfig` is upgraded.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"
        "###
        );
    });

    Ok(())
}

/// Respect locked versions with `uv lock`, unless `--upgrade` is passed.
#[test]
fn lock_git_sha() -> Result<()> {
    let context = TestContext::new("3.12");

    // Lock to a specific commit on `main`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
        sdist = { url = "https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Rewrite the lockfile, as if it were locked against `main`.
    let lock = lock.replace("rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979", "rev=main");
    fs_err::write(context.temp_dir.join("uv.lock"), lock)?;

    // Lock `anyio` against `main`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@main"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    // The lockfile should resolve to `0dacfd662c64cb4ceb16e6cf65a157a8b715b979`, even though it's
    // not the latest commit on `main`.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
        sdist = { url = "https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Relock with `--upgrade`.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade-package").arg("uv-public-pypackage"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    // The lockfile should resolve to `b270df1a2fb5d012294e9aaf05e7e0bab1e6a389`, the latest commit
    // on `main`.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389"
        sdist = { url = "https://github.com/astral-test/uv-public-pypackage?rev=main#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    Ok(())
}
