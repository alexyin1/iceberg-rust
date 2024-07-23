// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use crate::cmd::{get_cmd_output, run_command};
use std::process::Command;
use std::net::{SocketAddr, is_unspecified};

#[derive(Debug)]
enum EngineProvider {
    Docker,
    Podman
}

/// A utility to manage the lifecycle of `docker compose`.
///
/// It will start `docker compose` when calling the `run` method and will be stopped via [`Drop`].
#[derive(Debug)]
pub struct DockerCompose {
    project_name: String,
    docker_compose_dir: String,
    engine_provider: EngineProvider
}

fn get_engine_provider() -> EngineProvider {
    let mut cmd = Command::new("docker");
    cmd.arg("--version");
    let vers_str = get_cmd_output(cmd, format!("Get engine provider"))
        .trim()
        .to_lowercase()
        .to_string();
    if vers_str.contains("podman") {
        EngineProvider::Podman
    } else {
        EngineProvider::Docker
    }
}

impl DockerCompose {
    pub fn new(project_name: impl ToString, docker_compose_dir: impl ToString) -> Self {
        Self {
            project_name: project_name.to_string(),
            docker_compose_dir: docker_compose_dir.to_string(),
            engine_provider: get_engine_provider()
        }
    }

    pub fn project_name(&self) -> &str {
        self.project_name.as_str()
    }

    fn get_os_arch() -> String {
        let mut cmd = Command::new("docker");
        cmd.arg("version")
            .arg("--format")
            .arg("{{.Server.OsArch}}");

        get_cmd_output(cmd, "Get os arch".to_string())
            .trim()
            .to_string()
    }

    pub fn run(&self) {
        let mut cmd = Command::new("docker");
        cmd.current_dir(&self.docker_compose_dir);

        cmd.env("DOCKER_DEFAULT_PLATFORM", Self::get_os_arch());

        cmd.args(vec![
            "compose",
            "-p",
            self.project_name.as_str(),
            "up",
            "-d",
            "--wait",
            "--timeout",
            "1200000",
        ]);

        run_command(
            cmd,
            format!(
                "Starting docker compose in {}, project name: {}",
                self.docker_compose_dir, self.project_name
            ),
        )
    }

    pub fn get_container_ip(&self, service_name: impl AsRef<str>) -> String {
        let container_name = format!("{}-{}-1", self.project_name, service_name.as_ref());
        let mut cmd = Command::new("docker");
        cmd.arg("inspect")
            .arg("-f")
            .arg("{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}")
            .arg(&container_name);

        get_cmd_output(cmd, format!("Get container ip of {container_name}"))
            .trim()
            .to_string()
    }

    pub fn get_mapped_container_socket(&self, service_name: impl AsRef<str>, unmapped_port: u16) -> (String, u16) {
        let container_name = format!("{}-{}-1", self.project_name, service_name.as_ref());
        let mut cmd = Command::new("docker");
        cmd.arg("port")
            .arg(&container_name)
            .arg(unmapped_port.to_string());

        let mapped_socket: SocketAddr = get_cmd_output(cmd, format!("Get port mapping for {container_name}"))
            .trim()
            .to_string()
            .parse()
            .expect("Unable to parse socket address");

        if mapped_socket.ip().is_unspecified() {
            (String::from("127.0.0.1"), mapped_socket.port())
        } else {
            (mapped_socket.ip().to_string(), mapped_socket.port())
        }
    }

    pub fn get_container_socket(&self, service_name: impl AsRef<str>, unmapped_port: u16) -> (String, u16) {
        match self.engine_provider {
            // docker containers always get an addressable IP, so no portforwarding
            EngineProvider::Docker => {
                (self.get_container_ip(service_name), unmapped_port)
            }
            // podman rootless containers don't get an IP by default.
            // Instead, they share host IP and forward container ports to the host.
            EngineProvider::Podman => {
                self.get_mapped_container_socket(service_name, unmapped_port)
            }
        }
    }
}

impl Drop for DockerCompose {
    fn drop(&mut self) {
        let mut cmd = Command::new("docker");
        cmd.current_dir(&self.docker_compose_dir);

        cmd.args(vec![
            "compose",
            "-p",
            self.project_name.as_str(),
            "down",
            "-v",
            "--remove-orphans",
        ]);

        run_command(
            cmd,
            format!(
                "Stopping docker compose in {}, project name: {}",
                self.docker_compose_dir, self.project_name
            ),
        )
    }
}
