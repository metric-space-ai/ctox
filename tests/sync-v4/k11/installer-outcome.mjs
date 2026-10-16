// A partial installer success must not hide a failed rollback or child failure.
export function completedTransition(record,roundtrip){
 if((roundtrip&&record.installer?.roundtripExecuted!==true)||record.failure||record.error)return false;
 return record.controllerExit===0&&record.controller?.groupAbsent===true&&record.installer?.installerExecuted===true;
}
