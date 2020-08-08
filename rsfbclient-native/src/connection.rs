//! `FirebirdConnection` implementation for the native fbclient

use rsfbclient_core::*;

use crate::{ibase::IBase, params::Params, row::ColumnBuffer, status::Status, xsqlda::XSqlDa};
use std::{collections::HashMap, convert::TryFrom, ptr};

/// Client that wraps the native fbclient library
pub struct NativeFbClient {
    host: String,
    port: u16,
    ibase: IBase,
    status: Status,
    /// Output xsqldas and column buffers for the prepared statements
    stmt_data_map: HashMap<ibase::isc_tr_handle, (XSqlDa, Vec<ColumnBuffer>)>,
}

impl NativeFbClient {
    #[cfg(not(feature = "dynamic_loading"))]
    pub fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            ibase: IBase,
            status: Default::default(),
            stmt_data_map: Default::default(),
        }
    }

    #[cfg(feature = "dynamic_loading")]
    pub fn new(host: String, port: u16, lib_path: String) -> Result<Self, FbError> {
        Self {
            host,
            port,
            ibase: IBase::new(lib_path).map_err(|e| FbError {
                code: -1,
                msg: e.to_string(),
            })?,
            status: Default::default(),
            xsqlda_map: Default::default(),
        }
    }
}

impl FirebirdClient for NativeFbClient {
    type DbHandle = ibase::isc_db_handle;
    type TrHandle = ibase::isc_tr_handle;
    type StmtHandle = ibase::isc_stmt_handle;

    fn attach_database(
        &mut self,
        db_name: &str,
        user: &str,
        pass: &str,
    ) -> Result<Self::DbHandle, FbError> {
        let mut handle = 0;

        let dpb = {
            let mut dpb: Vec<u8> = Vec::with_capacity(64);

            dpb.extend(&[ibase::isc_dpb_version1 as u8]);

            dpb.extend(&[ibase::isc_dpb_user_name as u8, user.len() as u8]);
            dpb.extend(user.bytes());

            dpb.extend(&[ibase::isc_dpb_password as u8, pass.len() as u8]);
            dpb.extend(pass.bytes());

            // Makes the database convert the strings to utf-8, allowing non ascii characters
            let charset = b"UTF8";

            dpb.extend(&[ibase::isc_dpb_lc_ctype as u8, charset.len() as u8]);
            dpb.extend(charset);

            dpb
        };

        let conn_string = format!("{}/{}:{}", self.host, self.port, db_name);

        unsafe {
            if self.ibase.isc_attach_database()(
                &mut self.status[0],
                conn_string.len() as i16,
                conn_string.as_ptr() as *const _,
                &mut handle,
                dpb.len() as i16,
                dpb.as_ptr() as *const _,
            ) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }
        }

        // Assert that the handle is valid
        debug_assert_ne!(handle, 0);

        Ok(handle)
    }

    fn detach_database(&mut self, db_handle: Self::DbHandle) -> Result<(), FbError> {
        let mut handle = db_handle;
        unsafe {
            // Close the connection, if the handle is valid
            if handle != 0
                && self.ibase.isc_detach_database()(&mut self.status[0], &mut handle) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }
        }
        Ok(())
    }

    fn drop_database(&mut self, db_handle: Self::DbHandle) -> Result<(), FbError> {
        let mut handle = db_handle;
        unsafe {
            if self.ibase.isc_drop_database()(&mut self.status[0], &mut handle) != 0 {
                return Err(self.status.as_error(&self.ibase));
            }
        }
        Ok(())
    }

    fn begin_transaction(
        &mut self,
        mut db_handle: Self::DbHandle,
        isolation_level: TrIsolationLevel,
    ) -> Result<Self::TrHandle, FbError> {
        let mut handle = 0;

        // Transaction parameter buffer
        let tpb = [ibase::isc_tpb_version3 as u8, isolation_level as u8];

        #[repr(C)]
        struct IscTeb {
            db_handle: *mut ibase::isc_db_handle,
            tpb_len: usize,
            tpb_ptr: *const u8,
        }

        unsafe {
            if self.ibase.isc_start_multiple()(
                &mut self.status[0],
                &mut handle,
                1,
                &mut IscTeb {
                    db_handle: &mut db_handle,
                    tpb_len: tpb.len(),
                    tpb_ptr: &tpb[0],
                } as *mut _ as _,
            ) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }
        }

        // Assert that the handle is valid
        debug_assert_ne!(handle, 0);

        Ok(handle)
    }

    fn transaction_operation(
        &mut self,
        tr_handle: Self::TrHandle,
        op: TrOp,
    ) -> Result<(), FbError> {
        let mut handle = tr_handle;
        unsafe {
            if match op {
                TrOp::Commit => {
                    self.ibase.isc_commit_transaction()(&mut self.status[0], &mut handle)
                }
                TrOp::CommitRetaining => {
                    self.ibase.isc_commit_retaining()(&mut self.status[0], &mut handle)
                }
                TrOp::Rollback => {
                    self.ibase.isc_rollback_transaction()(&mut self.status[0], &mut handle)
                }
                TrOp::RollbackRetaining => {
                    self.ibase.isc_rollback_retaining()(&mut self.status[0], &mut handle)
                }
            } != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }
        }
        Ok(())
    }

    fn exec_immediate(
        &mut self,
        mut db_handle: Self::DbHandle,
        mut tr_handle: Self::TrHandle,
        dialect: Dialect,
        sql: &str,
    ) -> Result<(), FbError> {
        unsafe {
            if self.ibase.isc_dsql_execute_immediate()(
                &mut self.status[0],
                &mut db_handle,
                &mut tr_handle,
                sql.len() as u16,
                sql.as_ptr() as *const _,
                dialect as u16,
                ptr::null(),
            ) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }
        }
        Ok(())
    }

    fn prepare_statement(
        &mut self,
        mut db_handle: Self::DbHandle,
        mut tr_handle: Self::TrHandle,
        dialect: Dialect,
        sql: &str,
    ) -> Result<(StmtType, Self::StmtHandle), FbError> {
        let mut handle = 0;

        let mut xsqlda = XSqlDa::new(1);

        let mut stmt_type = 0;

        unsafe {
            if self.ibase.isc_dsql_allocate_statement()(
                &mut self.status[0],
                &mut db_handle,
                &mut handle,
            ) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }

            if self.ibase.isc_dsql_prepare()(
                &mut self.status[0],
                &mut tr_handle,
                &mut handle,
                sql.len() as u16,
                sql.as_ptr() as *const _,
                dialect as u16,
                &mut *xsqlda,
            ) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }

            let row_count = xsqlda.sqld;

            if row_count > xsqlda.sqln {
                // Need more XSQLVARs
                xsqlda = XSqlDa::new(row_count);

                if self.ibase.isc_dsql_describe()(&mut self.status[0], &mut handle, 1, &mut *xsqlda)
                    != 0
                {
                    return Err(self.status.as_error(&self.ibase));
                }
            }

            // Get the statement type
            let info_req = [ibase::isc_info_sql_stmt_type as i8];
            let mut info_buf = [0; 10];

            if self.ibase.isc_dsql_sql_info()(
                &mut self.status[0],
                &mut handle,
                info_req.len() as i16,
                &info_req[0],
                info_buf.len() as i16,
                &mut info_buf[0],
            ) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }

            for &v in &info_buf[3..] {
                // Search for the data
                if v != 0 {
                    stmt_type = v;
                    break;
                }
            }
        }

        let stmt_type = StmtType::try_from(stmt_type as u8).map_err(|_| FbError {
            code: -1,
            msg: format!("Invalid statement type: {}", stmt_type),
        })?;

        // Create the column buffers and set the xsqlda conercions
        let col_buffers = (0..xsqlda.sqld)
            .map(|col| {
                let xcol = xsqlda.get_xsqlvar_mut(col as usize).unwrap();

                ColumnBuffer::from_xsqlvar(xcol)
            })
            .collect::<Result<_, _>>()?;

        self.stmt_data_map.insert(handle, (xsqlda, col_buffers));

        Ok((stmt_type, handle))
    }

    fn free_statement(
        &mut self,
        mut stmt_handle: Self::StmtHandle,
        op: FreeStmtOp,
    ) -> Result<(), FbError> {
        unsafe {
            if self.ibase.isc_dsql_free_statement()(
                &mut self.status[0],
                &mut stmt_handle,
                op as u16,
            ) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }
        }

        if op == FreeStmtOp::Drop {
            self.stmt_data_map.remove(&stmt_handle);
        }

        Ok(())
    }

    fn execute(
        &mut self,
        mut tr_handle: Self::TrHandle,
        mut stmt_handle: Self::StmtHandle,
        params: Vec<Param>,
    ) -> Result<(), FbError> {
        let params = Params::new(&self.ibase, &mut self.status, &mut stmt_handle, params)?;

        unsafe {
            if self.ibase.isc_dsql_execute()(
                &mut self.status[0],
                &mut tr_handle,
                &mut stmt_handle,
                1,
                if let Some(xsqlda) = &params.xsqlda {
                    &**xsqlda
                } else {
                    ptr::null()
                },
            ) != 0
            {
                return Err(self.status.as_error(&self.ibase));
            }
        }

        // Just to make sure the params are not dropped too soon
        drop(params);

        Ok(())
    }

    fn fetch(&mut self, mut stmt_handle: Self::StmtHandle) -> Result<Option<Vec<Column>>, FbError> {
        let (xsqlda, col_buf) = self
            .stmt_data_map
            .get(&stmt_handle)
            .ok_or_else(|| FbError {
                code: -1,
                msg: "Tried to fetch a dropped statement".into(),
            })?;

        unsafe {
            let fetch_status =
                self.ibase.isc_dsql_fetch()(&mut self.status[0], &mut stmt_handle, 1, &**xsqlda);

            // 100 indicates that no more rows: http://docwiki.embarcadero.com/InterBase/2020/en/Isc_dsql_fetch()
            if fetch_status == 100 {
                return Ok(None);
            }

            if fetch_status != 0 {
                return Err(self.status.as_error(&self.ibase));
            };
        }

        let cols = col_buf
            .iter()
            .map(|cb| cb.to_column())
            .collect::<Result<_, _>>()?;

        Ok(Some(cols))
    }
}
