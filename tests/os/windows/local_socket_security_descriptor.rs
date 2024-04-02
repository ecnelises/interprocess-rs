use crate::{
	local_socket::{traits::Stream as _, ListenerOptions, Stream},
	os::windows::{
		local_socket::ListenerOptionsExt, AsRawHandleExt as _, AsSecurityDescriptorExt,
		BorrowedSecurityDescriptor, LocalBox, SecurityDescriptor,
	},
	tests::util::*,
	TryClone,
};
use std::{io, os::windows::prelude::*, ptr, sync::Arc};
use widestring::{U16CStr, U16Str};
use windows_sys::Win32::{
	Foundation::STATUS_SUCCESS,
	Security::{
		Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT, SE_OBJECT_TYPE},
		DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
	},
};

const SECINFO: u32 =
	DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION;

fn get_sd(handle: BorrowedHandle<'_>, ot: SE_OBJECT_TYPE) -> TestResult<SecurityDescriptor> {
	let mut sdptr = ptr::null_mut();
	let errno = unsafe {
		GetSecurityInfo(
			handle.as_int_handle(),
			ot,
			SECINFO,
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			&mut sdptr,
		) as i32
	};
	(errno == STATUS_SUCCESS)
		.then_some(())
		.ok_or_else(|| io::Error::from_raw_os_error(errno))
		.opname("GetSecurityInfo")?;

	let sdbx = unsafe { LocalBox::from_raw(sdptr) };
	unsafe { BorrowedSecurityDescriptor::from_ptr(sdbx.as_ptr()) }
		.to_owned_sd()
		.opname("security descriptor clone")
}

fn get_process_sd() -> TestResult<SecurityDescriptor> {
	get_sd(
		unsafe { BorrowedHandle::borrow_raw(-1 as _) },
		SE_KERNEL_OBJECT,
	)
	.opname("get_process_sd()")
}

#[test]
fn local_socket_security_descriptor() -> TestResult {
	let sd = get_process_sd()?;
	let (name, listener) =
		listen_and_pick_name(&mut namegen_local_socket(make_id!(), false), |nm| {
			ListenerOptions::new()
				.name(nm.borrow())
				.security_descriptor(sd.try_clone()?)
				.create_sync()
		})?;
	let _ = Stream::connect(Arc::try_unwrap(name).unwrap()).opname("client connect")?;

	let listener_handle = OwnedHandle::from(listener);
	let listener_sd =
		get_sd(listener_handle.as_handle(), SE_KERNEL_OBJECT).opname("get listener SD")?;

	sd.serialize(SECINFO, |old_s| {
		listener_sd.serialize(SECINFO, |new_s| {
			let start = ensure_equal_non_acl_part(old_s, new_s)?;
			ensure_equal_number_of_opening_parentheses(&old_s[start..], &new_s[start..])?;
			TestResult::Ok(())
		})
	})
	.opname("serialize")???;

	Ok(())
}

fn ensure_equal_non_acl_part(a: &U16CStr, b: &U16CStr) -> TestResult<usize> {
	let mut idx = 0;
	for (i, (ca, cb)) in a
		.as_slice()
		.iter()
		.copied()
		.zip(b.as_slice().iter().copied())
		.enumerate()
	{
		idx = i;
		if ca == 'D' as u16 {
			break;
		}
		ensure_eq!(ca, cb);
	}
	Ok(idx)
}

fn count_opening_parentheses(s: &U16Str) -> u32 {
	let mut cpa = 0;
	for c in s.as_slice().iter().copied() {
		if c == '(' as u16 {
			cpa += 1;
		}
	}
	cpa
}

fn ensure_equal_number_of_opening_parentheses(a: &U16Str, b: &U16Str) -> TestResult {
	ensure_eq!(count_opening_parentheses(a), count_opening_parentheses(b));
	Ok(())
}