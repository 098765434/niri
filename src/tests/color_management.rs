//! wp-color-management-v1 from a real client: every request that makes an object, and the replies
//! that end by destroying one (information's `done`), which once crashed niri inside the request.

use std::sync::Mutex;

use smithay::reexports::wayland_protocols::wp::color_management::v1::client::{
    wp_color_management_output_v1::{self, WpColorManagementOutputV1},
    wp_color_management_surface_feedback_v1::{self, WpColorManagementSurfaceFeedbackV1},
    wp_color_management_surface_v1::{self, WpColorManagementSurfaceV1},
    wp_color_manager_v1::{self, WpColorManagerV1},
    wp_image_description_creator_params_v1::{self, WpImageDescriptionCreatorParamsV1},
    wp_image_description_info_v1::{self, WpImageDescriptionInfoV1},
    wp_image_description_v1::{self, WpImageDescriptionV1},
};
use wayland_client::{Connection, Dispatch, QueueHandle};

use super::*;
use crate::tests::client::State;

/// What the client heard, in order.
static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn heard(what: impl Into<String>) {
    HEARD.lock().unwrap().push(what.into());
}

fn take() -> Vec<String> {
    std::mem::take(&mut *HEARD.lock().unwrap())
}

#[test]
fn information_and_descriptions() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let id = f.add_client();
    f.roundtrip(id);
    take();

    let client = f.client(id);
    let qh = client.qh.clone();
    let manager = client
        .state
        .color_manager
        .clone()
        .expect("niri advertises it");
    let output = client.state.outputs.keys().next().unwrap().clone();

    // an output's description, and its information
    let cm_output = manager.get_output(&output, &qh, ());
    let description = cm_output.get_image_description(&qh, ());
    f.roundtrip(id);
    assert!(take().iter().any(|e| e.starts_with("ready")));

    description.get_information(&qh, ());
    f.double_roundtrip(id);
    let info = take();
    assert!(info.contains(&String::from("primaries")), "{info:?}");
    assert_eq!(info.last().map(String::as_str), Some("done"), "{info:?}");

    // a parametric HDR10 description, set on a surface
    let client = f.client(id);
    let surface = client
        .state
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let params = manager.create_parametric_creator(&qh, ());
    params.set_tf_named(wp_color_manager_v1::TransferFunction::St2084Pq);
    params.set_primaries_named(wp_color_manager_v1::Primaries::Bt2020);
    params.set_max_cll(1000);
    let hdr10 = params.create(&qh, ());
    let cm_surface = manager.get_surface(&surface, &qh, ());
    let feedback = manager.get_surface_feedback(&surface, &qh, ());
    f.roundtrip(id);
    assert!(take().iter().any(|e| e.starts_with("ready")));

    cm_surface.set_image_description(&hdr10, wp_color_manager_v1::RenderIntent::Perceptual);
    surface.commit();
    let _preferred = feedback.get_preferred(&qh, ());
    let _scrgb = manager.create_windows_scrgb(&qh, ());
    f.double_roundtrip(id);
    assert_eq!(take().iter().filter(|e| e.starts_with("ready")).count(), 2);
}

impl Dispatch<WpColorManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpColorManagerV1,
        _: wp_color_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpColorManagementOutputV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpColorManagementOutputV1,
        _: wp_color_management_output_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        heard("output changed");
    }
}

impl Dispatch<WpColorManagementSurfaceV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpColorManagementSurfaceV1,
        _: wp_color_management_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpColorManagementSurfaceFeedbackV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpColorManagementSurfaceFeedbackV1,
        _: wp_color_management_surface_feedback_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        heard("preferred changed");
    }
}

impl Dispatch<WpImageDescriptionCreatorParamsV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpImageDescriptionCreatorParamsV1,
        _: wp_image_description_creator_params_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpImageDescriptionV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpImageDescriptionV1,
        event: wp_image_description_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wp_image_description_v1::Event::Ready { .. }
            | wp_image_description_v1::Event::Ready2 { .. } => heard("ready"),
            wp_image_description_v1::Event::Failed { msg, .. } => heard(format!("failed {msg}")),
            _ => (),
        }
    }
}

impl Dispatch<WpImageDescriptionInfoV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpImageDescriptionInfoV1,
        event: wp_image_description_info_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        heard(match event {
            wp_image_description_info_v1::Event::Primaries { .. } => "primaries",
            wp_image_description_info_v1::Event::Done => "done",
            _ => "info",
        });
    }
}
