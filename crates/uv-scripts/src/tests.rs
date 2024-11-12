use crate::{serialize_metadata, Pep723Error, ScriptTag};

#[test]
fn missing_space() {
    let contents = indoc::indoc! {r"
        # /// script
        #requires-python = '>=3.11'
        # ///
    "};

    assert!(matches!(
        ScriptTag::parse(contents.as_bytes()),
        Err(Pep723Error::UnclosedBlock)
    ));
}

#[test]
fn no_closing_pragma() {
    let contents = indoc::indoc! {r"
        # /// script
        # requires-python = '>=3.11'
        # dependencies = [
        #     'requests<3',
        #     'rich',
        # ]
    "};

    assert!(matches!(
        ScriptTag::parse(contents.as_bytes()),
        Err(Pep723Error::UnclosedBlock)
    ));
}

#[test]
fn leading_content() {
    let contents = indoc::indoc! {r"
        pass # /// script
        # requires-python = '>=3.11'
        # dependencies = [
        #   'requests<3',
        #   'rich',
        # ]
        # ///
        #
        #
    "};

    assert_eq!(ScriptTag::parse(contents.as_bytes()).unwrap(), None);
}

#[test]
fn simple() {
    let contents = indoc::indoc! {r"
        # /// script
        # requires-python = '>=3.11'
        # dependencies = [
        #     'requests<3',
        #     'rich',
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get('https://peps.python.org/api/peps.json')
        data = resp.json()
    "};

    let expected_metadata = indoc::indoc! {r"
        requires-python = '>=3.11'
        dependencies = [
            'requests<3',
            'rich',
        ]
    "};

    let expected_data = indoc::indoc! {r"

        import requests
        from rich.pretty import pprint

        resp = requests.get('https://peps.python.org/api/peps.json')
        data = resp.json()
    "};

    let actual = ScriptTag::parse(contents.as_bytes()).unwrap().unwrap();

    assert_eq!(actual.prelude, String::new());
    assert_eq!(actual.metadata, expected_metadata);
    assert_eq!(actual.postlude, expected_data);
}

#[test]
fn simple_with_shebang() {
    let contents = indoc::indoc! {r"
        #!/usr/bin/env python3
        # /// script
        # requires-python = '>=3.11'
        # dependencies = [
        #     'requests<3',
        #     'rich',
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get('https://peps.python.org/api/peps.json')
        data = resp.json()
    "};

    let expected_metadata = indoc::indoc! {r"
        requires-python = '>=3.11'
        dependencies = [
            'requests<3',
            'rich',
        ]
    "};

    let expected_data = indoc::indoc! {r"

        import requests
        from rich.pretty import pprint

        resp = requests.get('https://peps.python.org/api/peps.json')
        data = resp.json()
    "};

    let actual = ScriptTag::parse(contents.as_bytes()).unwrap().unwrap();

    assert_eq!(actual.prelude, "#!/usr/bin/env python3\n".to_string());
    assert_eq!(actual.metadata, expected_metadata);
    assert_eq!(actual.postlude, expected_data);
}
#[test]
fn embedded_comment() {
    let contents = indoc::indoc! {r"
        # /// script
        # embedded-csharp = '''
        # /// <summary>
        # /// text
        # ///
        # /// </summary>
        # public class MyClass { }
        # '''
        # ///
    "};

    let expected = indoc::indoc! {r"
        embedded-csharp = '''
        /// <summary>
        /// text
        ///
        /// </summary>
        public class MyClass { }
        '''
    "};

    let actual = ScriptTag::parse(contents.as_bytes())
        .unwrap()
        .unwrap()
        .metadata;

    assert_eq!(actual, expected);
}

#[test]
fn trailing_lines() {
    let contents = indoc::indoc! {r"
        # /// script
        # requires-python = '>=3.11'
        # dependencies = [
        #     'requests<3',
        #     'rich',
        # ]
        # ///
        #
        #
    "};

    let expected = indoc::indoc! {r"
        requires-python = '>=3.11'
        dependencies = [
            'requests<3',
            'rich',
        ]
    "};

    let actual = ScriptTag::parse(contents.as_bytes())
        .unwrap()
        .unwrap()
        .metadata;

    assert_eq!(actual, expected);
}

#[test]
fn test_serialize_metadata_formatting() {
    let metadata = indoc::indoc! {r"
        requires-python = '>=3.11'
        dependencies = [
          'requests<3',
          'rich',
        ]
    "};

    let expected_output = indoc::indoc! {r"
        # /// script
        # requires-python = '>=3.11'
        # dependencies = [
        #   'requests<3',
        #   'rich',
        # ]
        # ///
    "};

    let result = serialize_metadata(metadata);
    assert_eq!(result, expected_output);
}

#[test]
fn test_serialize_metadata_empty() {
    let metadata = "";
    let expected_output = "# /// script\n# ///\n";

    let result = serialize_metadata(metadata);
    assert_eq!(result, expected_output);
}
