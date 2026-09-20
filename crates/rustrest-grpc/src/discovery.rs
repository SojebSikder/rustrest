//! Server reflection: walks the `grpc.reflection.v1.ServerReflection` (or,
//! as a fallback, the older `grpc.reflection.v1alpha.ServerReflection`)
//! service to discover what services/methods a target exposes and pull down
//! the raw `FileDescriptorProto`s needed to build a real descriptor pool for
//! them, without the caller ever supplying a `.proto` file.

use crate::client::{self, MethodKind};
use crate::descriptor;
use http::uri::PathAndQuery;
use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};
use prost_types::{FileDescriptorProto, FileDescriptorSet};
use std::time::Duration;
use tokio::sync::mpsc;
use tonic::transport::Channel;

/// How long to wait for one reflection round-trip before giving up - so a
/// server that accepts the stream but never replies (e.g. because it
/// doesn't actually implement the reflection service we guessed) fails
/// with a clear error instead of leaving the UI stuck on "Discovering…".
const REFLECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// Discovers every service/method the target exposes via reflection, trying
/// the modern `grpc.reflection.v1` package first and falling back to the
/// deprecated `grpc.reflection.v1alpha` one if that fails - servers have
/// been migrating between the two for a while, and which one (or both) is
/// registered varies.
pub async fn discover_via_reflection(
    channel: Channel,
) -> Result<(DescriptorPool, Vec<String>), String> {
    match walk_reflection(channel.clone(), "grpc.reflection.v1").await {
        Ok(result) => Ok(result),
        Err(v1_err) => match walk_reflection(channel, "grpc.reflection.v1alpha").await {
            Ok(result) => Ok(result),
            Err(v1alpha_err) => Err(format!(
                "Reflection failed on both grpc.reflection.v1 ({v1_err}) and \
                 grpc.reflection.v1alpha ({v1alpha_err})"
            )),
        },
    }
}

/// One round-trip against the reflection service: sends `request_msg`,
/// returns the single response it gets back (or times out).
async fn reflection_call(
    channel: Channel,
    package: &str,
    request_msg: DynamicMessage,
    response_desc: prost_reflect::MessageDescriptor,
) -> Result<DynamicMessage, String> {
    let path: PathAndQuery = format!("/{package}.ServerReflection/ServerReflectionInfo")
        .parse()
        .map_err(|e| format!("Invalid reflection path: {e}"))?;
    let (tx, mut rx) = mpsc::channel(4);

    tokio::time::timeout(
        REFLECTION_TIMEOUT,
        client::invoke(
            channel,
            path,
            response_desc,
            MethodKind::Bidi,
            vec![request_msg],
            Vec::new(),
            tx,
        ),
    )
    .await
    .map_err(|_| "Timed out waiting for a reflection response".to_string())?;

    rx.recv()
        .await
        .ok_or_else(|| "Server closed the reflection stream with no response".to_string())?
}

async fn walk_reflection(
    channel: Channel,
    package: &str,
) -> Result<(DescriptorPool, Vec<String>), String> {
    let reflection_pool = descriptor::reflection_pool()?;
    let request_desc = reflection_pool
        .get_message_by_name(&format!("{package}.ServerReflectionRequest"))
        .ok_or("Missing ServerReflectionRequest descriptor")?;
    let response_desc = reflection_pool
        .get_message_by_name(&format!("{package}.ServerReflectionResponse"))
        .ok_or("Missing ServerReflectionResponse descriptor")?;

    let mut list_req = DynamicMessage::new(request_desc.clone());
    list_req.set_field_by_name("list_services", Value::String(String::new()));

    let list_resp =
        reflection_call(channel.clone(), package, list_req, response_desc.clone()).await?;
    let services_msg = list_resp
        .get_field_by_name("list_services_response")
        .and_then(|v| match v.into_owned() {
            Value::Message(m) => Some(m),
            _ => None,
        })
        .ok_or("Server did not return a service list (is reflection enabled?)")?;

    let mut service_names = Vec::new();
    if let Some(list) = services_msg.get_field_by_name("service")
        && let Value::List(items) = list.into_owned()
    {
        for item in items {
            if let Value::Message(svc) = item
                && let Some(name) = svc.get_field_by_name("name")
                && let Value::String(s) = name.into_owned()
            {
                service_names.push(s);
            }
        }
    }

    let mut files: Vec<FileDescriptorProto> = Vec::new();
    let mut seen_files = std::collections::HashSet::new();

    for service_name in &service_names {
        let mut symbol_req = DynamicMessage::new(request_desc.clone());
        symbol_req.set_field_by_name(
            "file_containing_symbol",
            Value::String(service_name.clone()),
        );

        let resp =
            reflection_call(channel.clone(), package, symbol_req, response_desc.clone()).await?;
        let file_resp = resp
            .get_field_by_name("file_descriptor_response")
            .and_then(|v| match v.into_owned() {
                Value::Message(m) => Some(m),
                _ => None,
            });
        let Some(file_resp) = file_resp else { continue };

        if let Some(protos) = file_resp.get_field_by_name("file_descriptor_proto")
            && let Value::List(items) = protos.into_owned()
        {
            for item in items {
                if let Value::Bytes(bytes) = item
                    && let Ok(fdp) = FileDescriptorProto::decode(bytes.as_ref())
                    && seen_files.insert(fdp.name.clone())
                {
                    files.push(fdp);
                }
            }
        }
    }

    let fds = FileDescriptorSet { file: files };
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .map_err(|e| format!("Failed to build descriptor pool from reflection: {e}"))?;

    Ok((pool, service_names))
}
