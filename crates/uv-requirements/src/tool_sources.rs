#[cfg(feature = "todo")]
pub(crate) fn find_package_and_workspace(current_dir: &Path) -> Result<(), ProjectStructureError> {
    let mut ancestors = current_dir.ancestors();
    let (pyproject_toml, project_root_dir) = loop {
        if let Some(dir) = ancestors.next() {
            let path = dir.join("pyproject.toml");
            match fs_err::read_to_string(&path) {
                Ok(pyproject_toml) => break (pyproject_toml, dir),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => return Err(ProjectStructureError::Io(err)),
            }
        } else {
            return Err(ProjectStructureError::NoPyprojectToml(
                current_dir.user_display().to_string(),
            ));
        }
    };

    let pyproject_toml: PyProjectToml = toml::from_str(&pyproject_toml).map_err(|err| {
        ProjectStructureError::Toml(
            project_root_dir
                .join("pyproject.toml")
                .user_display()
                .to_string(),
            err,
        )
    })?;

    let workspace_definition = pyproject_toml
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref())
        .and_then(|uv| uv.workspace.as_ref())
        .map(|workspace| (project_root_dir, workspace.clone()));

    while let Some(dir) = ancestors.next() {
        let path = dir.join("pyproject.toml");
        let (pyproject_toml, _workspace_root_dir) = match fs_err::read_to_string(&path) {
            Ok(pyproject_toml) => (pyproject_toml, dir),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(ProjectStructureError::Io(err)),
        };

        let pyproject_toml: PyProjectToml = toml::from_str(&pyproject_toml).map_err(|err| {
            ProjectStructureError::Toml(
                project_root_dir
                    .join("pyproject.toml")
                    .user_display()
                    .to_string(),
                err,
            )
        })?;

        let workspace_definition_higher = pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
            .map(|workspace| (project_root_dir, workspace.clone()));

        if let (Some((project_root, _)), Some((_path_b, workspace_b))) =
            (&workspace_definition, &workspace_definition_higher)
        {
            if workspace_b
                .exclude
                .clone()
                .unwrap_or_default()
                .iter()
                .any(|exclude| exclude.matches_path(project_root))
            {
                todo!("Ok we're done this is good");
            } else if workspace_b
                .members
                .clone()
                .unwrap_or_default()
                .iter()
                .any(|exclude| exclude.matches_path(project_root))
            {
            } else {
                todo!("Error: Secret third thing");
            }
        }
    }

    Ok(())
}
