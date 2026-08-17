#[derive(Debug, Clone, PartialEq)]
pub enum ProgrammingLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    Go,
    Java,
    Kotlin,
    Swift,
    Dart,
    CSharp,
    Ruby,
    Php,
    Unknown,
}

impl ProgrammingLanguage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::Kotlin => "Kotlin",
            Self::Swift => "Swift",
            Self::Dart => "Dart",
            Self::CSharp => "C#",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Unknown => "Unknown",
        }
    }
}

pub fn detect_from_file(path: &str) -> ProgrammingLanguage {
    let path = path.to_lowercase();

    if path.ends_with(".rs") || path.ends_with("cargo.toml") {
        return ProgrammingLanguage::Rust;
    }

    if path.ends_with(".py")
        || path.ends_with("pyproject.toml")
        || path.ends_with("requirements.txt")
    {
        return ProgrammingLanguage::Python;
    }

    if path.ends_with(".ts") || path.ends_with("tsconfig.json") {
        return ProgrammingLanguage::TypeScript;
    }

    if path.ends_with(".js") || path.ends_with("package.json") {
        return ProgrammingLanguage::JavaScript;
    }

    if path.ends_with(".c") {
        return ProgrammingLanguage::C;
    }

    if path.ends_with(".cpp") || path.ends_with(".cc") || path.ends_with(".hpp") {
        return ProgrammingLanguage::Cpp;
    }

    if path.ends_with(".go") || path.ends_with("go.mod") {
        return ProgrammingLanguage::Go;
    }

    if path.ends_with(".java") || path.ends_with("pom.xml") {
        return ProgrammingLanguage::Java;
    }

    if path.ends_with(".kt") || path.ends_with(".kts") || path.ends_with("build.gradle") {
        return ProgrammingLanguage::Kotlin;
    }

    if path.ends_with(".swift") || path.ends_with("package.swift") {
        return ProgrammingLanguage::Swift;
    }

    if path.ends_with(".dart") || path.ends_with("pubspec.yaml") {
        return ProgrammingLanguage::Dart;
    }

    if path.ends_with(".cs") || path.ends_with(".sln") {
        return ProgrammingLanguage::CSharp;
    }

    if path.ends_with(".rb") {
        return ProgrammingLanguage::Ruby;
    }

    if path.ends_with(".php") {
        return ProgrammingLanguage::Php;
    }

    ProgrammingLanguage::Unknown
}
