package generated.routes;

import org.apache.camel.builder.RouteBuilder;
import org.apache.camel.LoggingLevel;
import jakarta.enterprise.context.ApplicationScoped;

@ApplicationScoped
public class RouterFlowRoute extends RouteBuilder {

    @Override
    public void configure() throws Exception {
        from("platform-http:/route?httpMethodRestrict=POST")
            .routeId("routerFlow")
            .choice()
                // TODO: review this condition, it is not converted (Mule: #[payload.type == 'A'])
                // - it reads a field of the payload: Mule reads it from JSON, XML, a Map or an object according to the MIME type of the payload, while Simple's ${body.field} calls a getter on a Java object and does not read a field out of a JSON or XML body; the payload type is not known to the migrator
                // - a message that reaches this condition fails here instead of taking a branch by guess; replace the predicate
                .when(exchange -> { throw new UnsupportedOperationException("choice condition not migrated: #[payload.type == 'A']"); })
                    .setBody(constant("Routed to handler A"))
                // TODO: review this condition, it is not converted (Mule: #[payload.type == 'B'])
                // - it reads a field of the payload: Mule reads it from JSON, XML, a Map or an object according to the MIME type of the payload, while Simple's ${body.field} calls a getter on a Java object and does not read a field out of a JSON or XML body; the payload type is not known to the migrator
                // - a message that reaches this condition fails here instead of taking a branch by guess; replace the predicate
                .when(exchange -> { throw new UnsupportedOperationException("choice condition not migrated: #[payload.type == 'B']"); })
                    .setBody(constant("Routed to handler B"))
                .otherwise()
                    .setBody(constant("Default handler"))
            .end()
            ;
    }
}
