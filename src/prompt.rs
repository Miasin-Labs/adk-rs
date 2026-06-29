#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentPrompt {
    role: String,
    task: Option<String>,
    input: Option<String>,
    tools: Vec<String>,
    constraints: Vec<String>,
    output: Option<String>,
}

impl AgentPrompt {
    pub fn new(role: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            task: None,
            input: None,
            tools: Vec::new(),
            constraints: Vec::new(),
            output: None,
        }
    }

    pub fn task(mut self, task: impl Into<String>) -> Self {
        self.task = Some(task.into());
        self
    }

    pub fn input(mut self, input: impl Into<String>) -> Self {
        self.input = Some(input.into());
        self
    }

    pub fn tools<I, T>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.tools = tools.into_iter().map(Into::into).collect();
        self
    }

    pub fn constraints<I, T>(mut self, constraints: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.constraints = constraints.into_iter().map(Into::into).collect();
        self
    }

    pub fn output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    pub fn render(&self) -> String {
        let mut rendered = format!("Role: {}", self.role);
        push_optional_section(&mut rendered, "Task", self.task.as_deref());
        push_optional_section(&mut rendered, "Input", self.input.as_deref());
        push_list_section(&mut rendered, "Tools", &self.tools);
        push_list_section(&mut rendered, "Constraints", &self.constraints);
        push_optional_section(&mut rendered, "Output", self.output.as_deref());
        rendered
    }
}

impl From<AgentPrompt> for String {
    fn from(prompt: AgentPrompt) -> Self {
        prompt.render()
    }
}

fn push_optional_section(rendered: &mut String, label: &str, value: Option<&str>) {
    if let Some(value) = value
        && !value.trim().is_empty()
    {
        rendered.push_str(&format!("\n\n{label}: {value}"));
    }
}

fn push_list_section(rendered: &mut String, label: &str, values: &[String]) {
    let values = values
        .iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return;
    }
    rendered.push_str(&format!("\n\n{label}:"));
    for value in values {
        rendered.push_str(&format!("\n- {value}"));
    }
}
