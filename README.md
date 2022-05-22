# servicerestarter

A Windows service whose primary task is to ensure one or more other Windows services are running.

## Operation

`servicerestarter` has a command line inspired by the new-style (> 1.0.8) [Apache Commons Procrun command line](https://commons.apache.org/proper/commons-daemon/procrun.html).

To install a `servicerestarter` service, run `servicerestarter install [SERVICENAME]` with the necessary privileges (generally Administrator).

To uninstall a service, `servicerestarter` or not, run `servicerestarter delete [SERVICENAME]` with the necessary privileges.

To start a service, run `servicerestarter start [SERVICENAME]` with the necessary privileges.

To stop a service, run `servicerestarter stop [SERVICENAME]` with the necessary privileges.

To run the service as a console application (instead of a Windows service), run `servicerestarter run [SERVICENAME]`. If no other mode is given, this is the default.

When `servicerestarter` is run as a service, it is run as `servicerestarter service SERVICENAME`. Since this causes functions to be called that are only available to Windows services, using `service` in the console does not make much sense.

If `SERVICENAME` is missing from the command line of any of the previous commands, the service name is taken from the name of the executable. The service name is used to find the parameters in the registry, which is why it is also used when running `servicerestarter` as a console application.

## Configuration

Configuration for the service is stored in the registry under `HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\[ServiceName]\Parameters` where `[ServiceName]` is the name of the service. It might be necessary to create this key first. The following options are understood:

* `ServicesExpectedRunning` (REG_MULTI_SZ, required): The names of the services that `servicerestarter` should take care of. If it finds, during its periodic checks, that a service is in the status _Stopped_, it will attempt to start it.

* `SleepDurationMilliseconds` (REG_DWORD or REG_QWORD, required): The amount of time, in milliseconds, that `servicerestarter` should wait between each status check of the services it is taking care of.

* `InitialSleepDurationMilliseconds` (REG_DWORD or REG_QWORD, optional): The amount of time, in milliseconds, that `servicerestarter` should wait before its initial status check of the services it is taking care of.

Here are the parameters for a `servicerestarter` instance unimaginatively named `servicerestarter` that checks every minute (60000 ms = 0xEA60 ms) if the services `node_exporter` and `jmx_exporter` are stopped and starts them:

    Windows Registry Editor Version 5.00
    
    [HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\servicerestarter\Parameters]
    "SleepDurationMilliseconds"=dword:0000ea60
    "ServicesExpectedRunning"=hex(7):6e,00,6f,00,64,00,65,00,5f,00,65,00,78,00,70,\
      00,6f,00,72,00,74,00,65,00,72,00,00,00,6a,00,6d,00,78,00,5f,00,65,00,78,00,\
      70,00,6f,00,72,00,74,00,65,00,72,00,00,00,00,00
