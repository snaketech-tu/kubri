# What depends on what

## Used by and deleting

**Used by** (`u`, in the menu of ConfigMaps, Secrets, PersistentVolumeClaims, ServiceAccounts, PriorityClasses and Services) lists what uses an object in its namespace:

- workloads, CronJobs, Jobs and bare pods whose pod spec names a ConfigMap, Secret, claim, ServiceAccount or PriorityClass, in volumes, projected volumes, environment variables, `envFrom` or `imagePullSecrets`
- Ingresses that serve TLS from a Secret
- Ingresses and Gateway API HTTPRoutes that send traffic to a Service

The delete dialog looks this up before you confirm. When something uses the object, it says so in red, for example "It is used by 3 Deployments and 1 Ingress for TLS", and names them, so you see what breaks or stops starting before you delete it. With several objects marked, each one is checked. When Kubri cannot look, for lack of permission or because the API does not answer, the dialog says nothing and the delete still works.
