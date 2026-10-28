;; The restore helper scripts/dev/restore.sh and reset-to-v0.sh deploy for one transaction: deploy
;; it, call restore, then put the account's code back. The actions share a receipt, so if any
;; fails nothing changes, and the helper never stays on the account. Dev only.
;;
;; restore writes and removes raw storage entries. Only the account itself can call it. Its
;; arguments are records, back to back, little-endian (restore_args in scripts/lib.sh builds them):
;;   1 <u32 key length> <key> <u32 value length> <value>   write the entry
;;   2 <u32 key length> <key>                              remove the entry
;;
;; restore.wasm is this file assembled. After editing it, rebuild that with
;;   wasm-as scripts/dev/restore.wat -o scripts/dev/restore.wasm     (binaryen)
;; or wasm-tools parse, and commit both.
(module
  (import "env" "input" (func $input (param i64)))
  (import "env" "register_len" (func $register_len (param i64) (result i64)))
  (import "env" "read_register" (func $read_register (param i64 i64)))
  (import "env" "predecessor_account_id" (func $predecessor_account_id (param i64)))
  (import "env" "current_account_id" (func $current_account_id (param i64)))
  (import "env" "storage_write" (func $storage_write (param i64 i64 i64 i64 i64) (result i64)))
  (import "env" "storage_remove" (func $storage_remove (param i64 i64 i64) (result i64)))
  (import "env" "panic_utf8" (func $panic_utf8 (param i64 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "only the account itself can restore")
  (data (i32.const 64) "bad args")

  ;; Account IDs are read to 128 and 192 (at most 64 bytes each), the arguments to 1024 onwards.
  (func $bad_args (call $panic_utf8 (i64.const 8) (i64.const 64)) (unreachable))

  (func (export "restore")
    (local $len i32) (local $i i32) (local $p i32) (local $end i32)
    (local $op i32) (local $key i32) (local $key_len i32) (local $value_len i32)

    ;; The caller must be the account itself.
    (call $predecessor_account_id (i64.const 0))
    (call $current_account_id (i64.const 1))
    (local.set $len (i32.wrap_i64 (call $register_len (i64.const 0))))
    (if (i32.ne (local.get $len) (i32.wrap_i64 (call $register_len (i64.const 1))))
      (then (call $panic_utf8 (i64.const 35) (i64.const 0)) (unreachable)))
    (call $read_register (i64.const 0) (i64.const 128))
    (call $read_register (i64.const 1) (i64.const 192))
    (block $same
      (loop $compare
        (br_if $same (i32.eq (local.get $i) (local.get $len)))
        (if (i32.ne (i32.load8_u (i32.add (i32.const 128) (local.get $i)))
                    (i32.load8_u (i32.add (i32.const 192) (local.get $i))))
          (then (call $panic_utf8 (i64.const 35) (i64.const 0)) (unreachable)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $compare)))

    ;; Read the arguments to 1024, growing memory to fit them.
    (call $input (i64.const 2))
    (local.set $len (i32.wrap_i64 (call $register_len (i64.const 2))))
    (drop (memory.grow (i32.add (i32.shr_u (local.get $len) (i32.const 16)) (i32.const 1))))
    (call $read_register (i64.const 2) (i64.const 1024))
    (local.set $p (i32.const 1024))
    (local.set $end (i32.add (i32.const 1024) (local.get $len)))

    (block $done
      (loop $record
        (br_if $done (i32.ge_u (local.get $p) (local.get $end)))
        (local.set $op (i32.load8_u (local.get $p)))
        (local.set $key_len (i32.load (i32.add (local.get $p) (i32.const 1))))
        (local.set $key (i32.add (local.get $p) (i32.const 5)))
        (local.set $p (i32.add (local.get $key) (local.get $key_len)))
        (if (i32.eq (local.get $op) (i32.const 1))
          (then
            (local.set $value_len (i32.load (local.get $p)))
            (drop (call $storage_write
              (i64.extend_i32_u (local.get $key_len)) (i64.extend_i32_u (local.get $key))
              (i64.extend_i32_u (local.get $value_len)) (i64.extend_i32_u (i32.add (local.get $p) (i32.const 4)))
              (i64.const 3)))
            (local.set $p (i32.add (local.get $p) (i32.add (i32.const 4) (local.get $value_len)))))
          (else
            (if (i32.ne (local.get $op) (i32.const 2)) (then (call $bad_args)))
            (drop (call $storage_remove
              (i64.extend_i32_u (local.get $key_len)) (i64.extend_i32_u (local.get $key)) (i64.const 3)))))
        (br $record)))
    ;; A record that runs past the end means the arguments were cut short.
    (if (i32.ne (local.get $p) (local.get $end)) (then (call $bad_args)))))
