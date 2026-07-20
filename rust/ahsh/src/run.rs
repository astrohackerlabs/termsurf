use crate::{
    command,
    config_files::{self, setup_config},
};
use log::trace;
#[cfg(feature = "plugin")]
use nu_cli::read_plugin_file;
use nu_cli::{EvaluateCommandsOpts, evaluate_commands, evaluate_file, evaluate_repl};
use nu_config::ConfigFileKind;
use nu_protocol::{
    PipelineData, ShellError, Spanned,
    engine::{EngineState, Stack},
    report_shell_error,
};
use nu_utils::perf;
use nu_utils::time::Instant;

pub(crate) fn run_commands(
    engine_state: &mut EngineState,
    mut stack: Stack,
    parsed_nu_cli_args: command::NushellCliArgs,
    use_color: bool,
    commands: &Spanned<String>,
    input: PipelineData,
    entire_start_time: nu_utils::time::Instant,
) {
    trace!("run_commands");

    let start_time = nu_utils::time::Instant::now();
    let create_scaffold = !engine_state.config_dirs.config_home.exists();

    if parsed_nu_cli_args.no_config_file.is_none() {
        #[cfg(feature = "plugin")]
        read_plugin_file(
            engine_state,
            parsed_nu_cli_args.plugin_file.as_ref().map(|s| s.span),
        );

        perf!("read plugins", start_time, use_color);

        let start_time = Instant::now();
        if engine_state.config_dirs.env_file.is_override()
            || parsed_nu_cli_args.login_shell.is_some()
        {
            config_files::read_config_file(
                engine_state,
                &mut stack,
                ConfigFileKind::Env,
                create_scaffold,
                true,
                parsed_nu_cli_args.env_file.as_ref(),
            );
        } else {
            config_files::read_default_env_file(engine_state, &mut stack)
        }

        perf!("read env.nu", start_time, use_color);

        let start_time = Instant::now();

        if engine_state.config_dirs.config_file.is_override()
            || parsed_nu_cli_args.login_shell.is_some()
        {
            config_files::read_config_file(
                engine_state,
                &mut stack,
                ConfigFileKind::Config,
                create_scaffold,
                true,
                parsed_nu_cli_args.config_file.as_ref(),
            );
        }

        perf!("read config.nu", start_time, use_color);

        let start_time = Instant::now();
        if parsed_nu_cli_args.login_shell.is_some() {
            config_files::read_loginshell_file(engine_state, &mut stack, false);
        }

        perf!("read login.nu", start_time, use_color);
    }

    engine_state.set_startup_time(entire_start_time.elapsed().as_nanos() as i64);
    engine_state.generate_nu_constant();

    let start_time = Instant::now();
    let result = evaluate_commands(
        commands,
        engine_state,
        &mut stack,
        input,
        EvaluateCommandsOpts {
            table_mode: parsed_nu_cli_args.table_mode,
            error_style: parsed_nu_cli_args.error_style,
            no_newline: parsed_nu_cli_args.no_newline.is_some(),
        },
    );
    perf!("evaluate_commands", start_time, use_color);

    if let Err(err) = result {
        // Match upstream nu: Exit must process::exit(code) without report_shell_error
        // (exit_code() maps Exit to 1 and the report message confuses users).
        if let ShellError::Exit { code, .. } = &err {
            std::process::exit(*code)
        }
        report_shell_error(Some(&stack), engine_state, &err);
        std::process::exit(err.exit_code().unwrap_or(0));
    }
}

pub(crate) fn run_file(
    engine_state: &mut EngineState,
    mut stack: Stack,
    parsed_nu_cli_args: command::NushellCliArgs,
    use_color: bool,
    script_name: String,
    args_to_script: Vec<String>,
    input: PipelineData,
) {
    trace!("run_file");

    if parsed_nu_cli_args.no_config_file.is_none() {
        let start_time = Instant::now();
        let create_scaffold = !engine_state.config_dirs.config_home.exists();
        #[cfg(feature = "plugin")]
        read_plugin_file(
            engine_state,
            parsed_nu_cli_args.plugin_file.as_ref().map(|s| s.span),
        );
        perf!("read plugins", start_time, use_color);

        let start_time = Instant::now();
        if engine_state.config_dirs.env_file.is_override() {
            config_files::read_config_file(
                engine_state,
                &mut stack,
                ConfigFileKind::Env,
                create_scaffold,
                true,
                parsed_nu_cli_args.env_file.as_ref(),
            );
        } else {
            config_files::read_default_env_file(engine_state, &mut stack)
        }
        perf!("read env.nu", start_time, use_color);

        let start_time = Instant::now();
        if engine_state.config_dirs.config_file.is_override() {
            config_files::read_config_file(
                engine_state,
                &mut stack,
                ConfigFileKind::Config,
                create_scaffold,
                true,
                parsed_nu_cli_args.config_file.as_ref(),
            );
        }
        perf!("read config.nu", start_time, use_color);
    }

    engine_state.generate_nu_constant();

    let start_time = Instant::now();
    let result = evaluate_file(
        script_name,
        &args_to_script,
        engine_state,
        &mut stack,
        input,
    );
    perf!("evaluate_file", start_time, use_color);

    if let Err(err) = result {
        if let ShellError::Exit { code, .. } = &err {
            std::process::exit(*code)
        }
        report_shell_error(Some(&stack), engine_state, &err);
        std::process::exit(err.exit_code().unwrap_or(0));
    }
}

pub(crate) fn run_repl(
    engine_state: &mut EngineState,
    mut stack: Stack,
    parsed_nu_cli_args: command::NushellCliArgs,
    entire_start_time: nu_utils::time::Instant,
) -> Result<(), miette::ErrReport> {
    trace!("run_repl");
    let start_time = nu_utils::time::Instant::now();

    // Create the dispatcher early — the persistent zsh subprocess initializes
    // via login (`zsh -l`) plus an explicit `.zshrc` source. Capture the
    // resulting env vars to inject into nushell's stack before config loading.
    let mut dispatcher = ahsh::dispatcher::ShannonDispatcher::new();
    if parsed_nu_cli_args.no_config_file.is_none() {
        for (key, value) in dispatcher.env_vars() {
            stack.add_env_var(
                key,
                nu_protocol::Value::string(value, nu_protocol::Span::unknown()),
            );
        }
    }

    if parsed_nu_cli_args.no_config_file.is_none() {
        setup_config(
            engine_state,
            &mut stack,
            parsed_nu_cli_args.login_shell.is_some(),
        );
    }

    let use_color = engine_state
        .get_config()
        .use_ansi_coloring
        .get(engine_state);
    perf!("setup_config", start_time, use_color);

    stack.add_env_var(
        "SHANNON_MODE".to_string(),
        nu_protocol::Value::string("nu", nu_protocol::Span::unknown()),
    );

    nu_cli::eval_source(
        engine_state,
        &mut stack,
        br#"$env.PROMPT_COMMAND = {||
            let mode = ($env.SHANNON_MODE? | default "nu")
            let color = match $mode {
                "nu" => (ansi green)
                "zsh" => (ansi cyan)
                _ => (ansi green)
            }
            let reset = (ansi reset)
            let dir = if ($env.PWD | str starts-with $env.HOME) {
                $env.PWD | str replace $env.HOME "~"
            } else {
                $env.PWD
            }
            $"($color)[($mode)](ansi reset) ($dir)"
        }"#,
        "ahsh-prompt",
        nu_protocol::PipelineData::empty(),
        false,
    );

    {
        use nu_protocol::BannerKind;
        let show_banner = engine_state.get_config().show_banner.clone();
        nu_cli::eval_source(
            engine_state,
            &mut stack,
            b"$env.config.show_banner = false",
            "ahsh-banner-disable",
            nu_protocol::PipelineData::empty(),
            false,
        );
        match show_banner {
            BannerKind::None => {}
            BannerKind::Short => {
                let green = "\x1b[32m";
                let bold = "\x1b[1m";
                let reset = "\x1b[0m";
                let fg = "\x1b[37m";
                eprintln!(
                    "{green}{bold}Startup Time:{reset}{fg} {:?}{reset}",
                    entire_start_time.elapsed()
                );
                eprintln!(
                    "{green}{bold}Shift+Tab:{reset}{fg} Nushell ↔ zsh{reset}"
                );
                eprintln!();
            }
            BannerKind::Full => {
                let version = env!("CARGO_PKG_VERSION");
                let nu_version = env!("NUSHELL_VERSION");
                let green = "\x1b[32m";
                let bold = "\x1b[1m";
                let reset = "\x1b[0m";
                let fg = "\x1b[37m";
                eprintln!(
                    "{fg}Welcome to {green}{bold}Astrohacker Shell{reset}{fg}, based on the {green}Nu{reset}{fg} language, where all data is structured!{reset}"
                );
                eprintln!(
                    "{fg}Version: {green}{version}{fg} (nushell {green}{nu_version}{fg}){reset}"
                );
                eprintln!(
                    "{fg}Press {green}{bold}Shift+Tab{reset}{fg} to toggle between Nushell and zsh.{reset}"
                );
                eprintln!(
                    "{green}{bold}Startup Time:{reset}{fg} {:?}{reset}",
                    entire_start_time.elapsed()
                );
                eprintln!();
            }
        }
    }

    let dispatcher: std::sync::Arc<std::sync::Mutex<Box<dyn nu_cli::ModeDispatcher>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Box::new(dispatcher)));

    let start_time = nu_utils::time::Instant::now();
    let ret_val = evaluate_repl(
        engine_state,
        stack,
        parsed_nu_cli_args.execute,
        parsed_nu_cli_args.no_std_lib,
        entire_start_time,
        Some(dispatcher),
    );
    perf!("evaluate_repl", start_time, use_color);

    ret_val
}
