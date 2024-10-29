use super::*;

#[test]
fn test_requires_txt() {
    let s = r"
Werkzeug>=0.14
Jinja2>=2.10

[dev]
pytest>=3
sphinx

[dotenv]
python-dotenv
    ";
    let meta = RequiresTxt::parse(s.as_bytes()).unwrap();
    assert_eq!(
        meta.requires_dist,
        vec![
            "Werkzeug>=0.14".parse().unwrap(),
            "Jinja2>=2.10".parse().unwrap(),
            "pytest>=3; extra == \"dev\"".parse().unwrap(),
            "sphinx; extra == \"dev\"".parse().unwrap(),
            "python-dotenv; extra == \"dotenv\"".parse().unwrap(),
        ]
    );

    let s = r"
Werkzeug>=0.14

[dev:]
Jinja2>=2.10

[:sys_platform == 'win32']
pytest>=3

[]
sphinx

[dotenv:sys_platform == 'darwin']
python-dotenv
    ";
    let meta = RequiresTxt::parse(s.as_bytes()).unwrap();
    assert_eq!(
        meta.requires_dist,
        vec![
            "Werkzeug>=0.14".parse().unwrap(),
            "Jinja2>=2.10 ; extra == \"dev\"".parse().unwrap(),
            "pytest>=3; sys_platform == 'win32'".parse().unwrap(),
            "sphinx".parse().unwrap(),
            "python-dotenv; sys_platform == 'darwin' and extra == \"dotenv\""
                .parse()
                .unwrap(),
        ]
    );
}
