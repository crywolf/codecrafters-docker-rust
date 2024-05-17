use std::path::PathBuf;

use anyhow::{Context, Result};
use bytes::{Buf, Bytes};
use flate2::read::GzDecoder;
use reqwest::{blocking::Client, header::ACCEPT};
use serde::Deserialize;

const AUTH_URL: &str = "https://auth.docker.io/token";
const REGISTRY_URL: &str = "https://registry.hub.docker.com/v2";
const MANIFEST_LIST_MEDIA_TYPE: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
const MANIFEST_MEDIA_TYPE: &str = "application/vnd.docker.distribution.manifest.v2+json";

#[allow(dead_code)]
struct Registry {
    client: Client,
    image: String,      // name:tag
    image_name: String, // name
    reference: String,  // tag
}

impl Registry {
    pub fn new(image: &str) -> Self {
        let (image_name, reference) = image.split_once(':').unwrap_or((image, "latest"));

        Self {
            client: Client::new(),
            image: image.to_owned(),
            image_name: image_name.to_owned(),
            reference: reference.to_owned(),
        }
    }

    pub fn authenticate(&self) -> Result<AuthenticatedRegistry> {
        let token = self.get_auth_token(&self.image_name)?.token;

        Ok(AuthenticatedRegistry::new(self, token))
    }

    fn get_auth_token(&self, image_name: &str) -> Result<AuthToken> {
        let scope = &format!("repository:library/{}:pull", image_name);
        let query = &[("service", "registry.docker.io"), ("scope", scope)];

        let token = self
            .client
            .get(AUTH_URL)
            .query(query)
            .send()
            .context("sending auth request")?
            .error_for_status()
            .context("requesting token")?
            .json()
            .context("parsing auth token")?;

        Ok(token)
    }
}

struct AuthenticatedRegistry<'a> {
    registry: &'a Registry,
    token: String,
}

impl<'a> AuthenticatedRegistry<'a> {
    pub fn new(registry: &'a Registry, token: String) -> Self {
        Self { registry, token }
    }

    fn get_manifest_list(&self) -> Result<Manifests> {
        let manifests = self
            .registry
            .client
            .get(format!(
                "{REGISTRY_URL}/library/{}/manifests/{}",
                self.registry.image_name, self.registry.reference
            ))
            .bearer_auth(&self.token)
            .header(ACCEPT, MANIFEST_LIST_MEDIA_TYPE)
            .send()
            .context("sending manifest list request")?
            .error_for_status()
            .context("requesting manifest list")?
            .json()
            .context("parsing image manifest list")?;

        Ok(manifests)
    }

    fn get_manifest(&self) -> Result<Manifest> {
        let manifests = self
            .registry
            .client
            .get(format!(
                "{REGISTRY_URL}/library/{}/manifests/{}",
                self.registry.image_name, self.registry.reference
            ))
            .bearer_auth(&self.token)
            .header(ACCEPT, MANIFEST_MEDIA_TYPE)
            .send()
            .context("sending manifest request")?
            .error_for_status()
            .context("requesting manifest")?
            .json()
            .context("parsing image manifest")?;

        Ok(manifests)
    }

    fn get_layer(&self, digest: &str) -> Result<Bytes> {
        let manifests = self
            .registry
            .client
            .get(format!(
                "{REGISTRY_URL}/library/{}/blobs/{}",
                self.registry.image_name, digest
            ))
            .bearer_auth(&self.token)
            .header(ACCEPT, MANIFEST_MEDIA_TYPE)
            .send()
            .context("sending layer request")?
            .error_for_status()
            .context("requesting image layer")?
            .bytes()?;

        Ok(manifests)
    }
}

pub fn pull_image(dir: &PathBuf, image: &str, architecture: &str, os: &str) -> Result<()> {
    let registry = Registry::new(image);

    let registry = registry.authenticate().context("authentication")?;

    let mut manifests = registry
        .get_manifest_list()
        .context("getting manifest list")?
        .manifests;

    manifests.retain(|m| m.platform.architecture == architecture && m.platform.os == os);

    if manifests.is_empty() {
        anyhow::bail!("no image manifest found for specified platform")
    };

    let manifest = registry.get_manifest().context("getting image manifest")?;

    if manifest.layers.is_empty() {
        anyhow::bail!("no image layers found")
    }

    for layer in manifest.layers.into_iter() {
        let layer_bytes = registry
            .get_layer(&layer.digest)
            .context("downloading image layer")?;

        if layer_bytes.len() != layer.size {
            anyhow::bail!(
                "image layer corrupted, expected {} bytes, got {} bytes",
                layer.size,
                layer_bytes.len()
            )
        }

        let decoder = GzDecoder::new(layer_bytes.reader());

        let mut archive = tar::Archive::new(decoder);
        archive.unpack(dir).context("unpacking image layer")?;
    }

    Ok(())
}

#[derive(Deserialize)]
struct AuthToken {
    token: String,
}

#[derive(Deserialize)]
struct Manifests {
    manifests: Vec<ManifestInList>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ManifestInList {
    digest: String,
    platform: Platform,
}

#[derive(Deserialize)]
struct Platform {
    architecture: String,
    os: String,
}

#[derive(Deserialize)]
struct Manifest {
    layers: Vec<Layer>,
}

#[derive(Deserialize)]
struct Layer {
    digest: String,
    size: usize,
}
