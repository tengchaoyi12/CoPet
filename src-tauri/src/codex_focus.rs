use tauri::AppHandle;

pub fn is_codex_bundle_identifier(value: Option<&str>) -> bool {
    matches!(value, Some("com.openai.codex"))
}

pub fn install_codex_focus_observer(app: &AppHandle) {
    install_native_codex_focus_observer(app);
}

#[cfg(target_os = "macos")]
fn install_native_codex_focus_observer(app: &AppHandle) {
    use block2::{DynBlock, RcBlock};
    use objc2_app_kit::{
        NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
        NSWorkspaceDidActivateApplicationNotification,
    };
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;
    use tauri::Manager;

    use crate::{emit_runtime_update, runtime_server::RuntimeManager};

    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();
    let app = app.clone();
    let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
        // SAFETY: NSNotificationCenter provides a valid notification pointer
        // for the duration of this callback.
        let notification = unsafe { notification.as_ref() };
        let bundle_identifier = notification.userInfo().and_then(|user_info| {
            // SAFETY: objc2 exposes the Objective-C workspace dictionary key
            // as an extern static; reading it is the intended binding usage.
            let application = user_info.objectForKey(unsafe { NSWorkspaceApplicationKey })?;
            let application = application.downcast_ref::<NSRunningApplication>()?;
            application
                .bundleIdentifier()
                .map(|identifier| identifier.to_string())
        });
        if !is_codex_bundle_identifier(bundle_identifier.as_deref()) {
            return;
        }

        let Some(runtime) = app.try_state::<RuntimeManager>() else {
            return;
        };
        emit_runtime_update(&app, runtime.clear_completed_task_notifications());
    });
    let block: &'static RcBlock<dyn Fn(NonNull<NSNotification>)> = Box::leak(Box::new(block));
    let block: &DynBlock<dyn Fn(NonNull<NSNotification>)> = block;

    // SAFETY: The block reference is leaked for process lifetime, so the
    // notification center never observes a dangling callback pointer.
    unsafe {
        let _ = center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
            None,
            block,
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn install_native_codex_focus_observer(_app: &AppHandle) {}
