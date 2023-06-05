// Copyright 2014-2021 The winit contributors
// Copyright 2021-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0

use crate::{platform::macos::ActivationPolicy, platform_impl::platform::app_state::AppState};

use cocoa::base::id;
use objc::{
  declare::ClassDecl,
  runtime::{Class, Object, Sel, BOOL, NO},
};
use std::{
  cell::{RefCell, RefMut},
  os::raw::c_void,
};

static AUX_DELEGATE_STATE_NAME: &str = "auxState";
/// Apple kInternetEventClass constant
#[allow(non_upper_case_globals)]
pub const kInternetEventClass: u32 = 0x4755524c;
/// Apple kAEGetURL constant
#[allow(non_upper_case_globals)]
pub const kAEGetURL: u32 = 0x4755524c;

// Global callback for rustdesk
extern "C" {
  fn handle_apple_event(obj: &Object, sel: Sel, event: u64, reply: u64) -> BOOL;
  fn service_should_handle_reopen(
    obj: &Object,
    sel: Sel,
    sender: id,
    hasVisibleWindows: BOOL,
  ) -> BOOL;
}

pub struct AuxDelegateState {
  /// We store this value in order to be able to defer setting the activation policy until
  /// after the app has finished launching. If the activation policy is set earlier, the
  /// menubar is initially unresponsive on macOS 10.15 for example.
  pub activation_policy: ActivationPolicy,

  pub create_default_menu: bool,

  pub activate_ignoring_other_apps: bool,
}

pub struct AppDelegateClass(pub *const Class);
unsafe impl Send for AppDelegateClass {}
unsafe impl Sync for AppDelegateClass {}

lazy_static! {
  pub static ref APP_DELEGATE_CLASS: AppDelegateClass = unsafe {
    let superclass = class!(NSResponder);
    let mut decl = ClassDecl::new("TaoAppDelegate", superclass).unwrap();

    decl.add_class_method(sel!(new), new as extern "C" fn(&Class, Sel) -> id);
    decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&Object, Sel));

    decl.add_method(
      sel!(applicationDidFinishLaunching:),
      did_finish_launching as extern "C" fn(&Object, Sel, id),
    );
    decl.add_method(
      sel!(applicationWillTerminate:),
      application_will_terminate as extern "C" fn(&Object, Sel, id),
    );
    decl.add_method(
      sel!(applicationWillBecomeActive:),
      application_will_become_active as extern "C" fn(&Object, Sel, id),
    );
    decl.add_method(
      sel!(handleEvent:withReplyEvent:),
      application_handle_apple_event as extern "C" fn(&Object, Sel, u64, u64) -> BOOL,
    );
    // decl.add_method(sel!(applicationShouldHandleReopen:hasVisibleWindows:), func)
    decl.add_method(sel!(applicationShouldHandleReopen:hasVisibleWindows:),
    application_should_handle_reopen as extern "C" fn (&Object, Sel, id, BOOL) -> BOOL);
    decl.add_ivar::<*mut c_void>(AUX_DELEGATE_STATE_NAME);

    AppDelegateClass(decl.register())
  };
}

/// Safety: Assumes that Object is an instance of APP_DELEGATE_CLASS
pub unsafe fn get_aux_state_mut(this: &Object) -> RefMut<'_, AuxDelegateState> {
  let ptr: *mut c_void = *this.get_ivar(AUX_DELEGATE_STATE_NAME);
  // Watch out that this needs to be the correct type
  (*(ptr as *mut RefCell<AuxDelegateState>)).borrow_mut()
}

extern "C" fn new(class: &Class, _: Sel) -> id {
  unsafe {
    let this: id = msg_send![class, alloc];
    let this: id = msg_send![this, init];
    (*this).set_ivar(
      AUX_DELEGATE_STATE_NAME,
      Box::into_raw(Box::new(RefCell::new(AuxDelegateState {
        activation_policy: ActivationPolicy::Regular,
        create_default_menu: true,
        activate_ignoring_other_apps: true,
      }))) as *mut c_void,
    );
    let cls = Class::get("NSAppleEventManager").unwrap();
    let manager: *mut Object = msg_send![cls, sharedAppleEventManager];
    let _: () = msg_send![manager,
      setEventHandler: this
      andSelector: sel!(handleEvent:withReplyEvent:)
      forEventClass: kInternetEventClass
      andEventID: kAEGetURL];
    this
  }
}

extern "C" fn dealloc(this: &Object, _: Sel) {
  unsafe {
    let state_ptr: *mut c_void = *(this.get_ivar(AUX_DELEGATE_STATE_NAME));
    // As soon as the box is constructed it is immediately dropped, releasing the underlying
    // memory
    Box::from_raw(state_ptr as *mut RefCell<AuxDelegateState>);
  }
}

extern "C" fn did_finish_launching(this: &Object, _: Sel, _: id) {
  trace!("Triggered `applicationDidFinishLaunching`");
  AppState::launched(this);
  trace!("Completed `applicationDidFinishLaunching`");
}

extern "C" fn application_will_terminate(_: &Object, _: Sel, _: id) {
  trace!("Triggered `applicationWillTerminate`");
  AppState::exit();
  trace!("Completed `applicationWillTerminate`");
}

extern "C" fn application_will_become_active(obj: &Object, sel: Sel, id: id) {
  trace!("Triggered `applicationWillBecomeActive`");
}

extern "C" fn application_handle_apple_event(
  _this: &Object,
  _cmd: Sel,
  event: u64,
  _reply: u64,
) -> BOOL {
  unsafe { handle_apple_event(_this, _cmd, event, _reply) }
}

extern "C" fn application_should_handle_reopen(
  obj: &Object,
  sel: Sel,
  id: id,
  has_visible_windows: BOOL,
) -> BOOL {
  unsafe { service_should_handle_reopen(obj, sel, id, has_visible_windows) }
}
